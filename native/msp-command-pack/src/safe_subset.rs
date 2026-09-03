//! Bounded, byte-oriented text utility commands for the virtual workspace.
//!
//! This module intentionally implements only the small ReadOS-owned subset of
//! `printf`, `head`, `tail`, and `wc`.  It does not use a host path, process,
//! environment, locale, or Unicode text stream.  File input is always obtained
//! through bounded `WorkspaceBackend::read_range` calls.

use super::*;
use std::cmp::min;

const FILE_READ_CHUNK_BYTES: usize = 32 * 1024;
const PRINTF_HELP: &[u8] = b"Usage: printf [--] FORMAT [ARGUMENT]...\n";
const PRINTF_VERSION: &[u8] = b"printf (ReadOS virtual coreutils-compatible) 1.0\n";

#[derive(Clone, Copy, Debug, Default)]
pub struct PrintfCommand;

impl Command for PrintfCommand {
    fn name(&self) -> &str {
        "printf"
    }

    fn summary(&self) -> Option<&str> {
        Some("format bytes without host or locale state")
    }

    fn run(
        &self,
        invocation: &CommandInvocation,
        _backend: &dyn WorkspaceBackend,
    ) -> CommandOutput {
        let args = invocation.args();
        if args.len() == 1 && args[0] == "--help" {
            return success_with_limits(PRINTF_HELP, invocation.limits());
        }
        if args.len() == 1 && args[0] == "--version" {
            return success_with_limits(PRINTF_VERSION, invocation.limits());
        }

        let format_index = usize::from(args.first().is_some_and(|arg| arg == "--"));
        let Some(format) = args.get(format_index) else {
            return failure_with_limits(
                2,
                b"printf: missing format operand\n",
                invocation.limits(),
            );
        };

        let mut output = OutputBuilder::new(invocation.limits());
        let mut argument_index = format_index.saturating_add(1);
        let format_bytes = format.as_bytes();
        let arguments = &args[argument_index..];
        let mut had_conversion = false;
        let mut status = 0;

        loop {
            let before = argument_index;
            let (consumed, stopped, converted, invalid_number) = match render_printf_once(
                format_bytes,
                arguments,
                &mut argument_index,
                format_index.saturating_add(1),
                &mut output,
            ) {
                Ok(result) => result,
                Err(()) => {
                    return failure_with_limits(2, b"printf: invalid format\n", invocation.limits())
                }
            };
            had_conversion |= converted;
            if invalid_number {
                status = 1;
            }
            if output.exceeded || stopped || !consumed || argument_index == before {
                break;
            }
            if argument_index
                >= format_index
                    .saturating_add(1)
                    .saturating_add(arguments.len())
            {
                break;
            }
        }

        // A format with no consuming conversion is deliberately rendered once,
        // even when extra arguments are supplied.
        let _ = had_conversion;
        let exceeded = output.exceeded;
        output.finish(if exceeded { 1 } else { status })
    }
}

fn render_printf_once(
    format: &[u8],
    arguments: &[String],
    argument_index: &mut usize,
    argument_base: usize,
    output: &mut OutputBuilder,
) -> Result<(bool, bool, bool, bool), ()> {
    let mut index = 0;
    let mut consumed = false;
    let mut converted = false;
    let mut invalid_number = false;
    while index < format.len() {
        match format[index] {
            b'\\' => {
                index += 1;
                let (bytes, stopped) = decode_escape_at(format, &mut index);
                if !output.stdout(&bytes) {
                    return Ok((consumed, true, converted, invalid_number));
                }
                if stopped {
                    return Ok((consumed, true, converted, invalid_number));
                }
            }
            b'%' => {
                index += 1;
                let spec = parse_printf_spec(format, &mut index)?;
                converted = true;
                if spec.conversion == b'%' {
                    if !output.stdout(b"%") {
                        return Ok((consumed, true, converted, invalid_number));
                    }
                    continue;
                }
                let value = arguments
                    .get(argument_index.saturating_sub(argument_base))
                    .map(String::as_bytes)
                    .unwrap_or_default();
                *argument_index = argument_index.saturating_add(1);
                consumed = true;
                let stopped = render_printf_conversion(&spec, value, output)?;
                if spec.conversion == b'd'
                    || spec.conversion == b'i'
                    || spec.conversion == b'u'
                    || spec.conversion == b'o'
                    || spec.conversion == b'x'
                    || spec.conversion == b'X'
                {
                    invalid_number |= !is_valid_printf_number(&spec, value);
                }
                if stopped {
                    return Ok((consumed, true, converted, invalid_number));
                }
            }
            byte => {
                if !output.stdout(&[byte]) {
                    return Ok((consumed, true, converted, invalid_number));
                }
                index += 1;
            }
        }
    }
    Ok((consumed, false, converted, invalid_number))
}

#[derive(Clone, Copy, Debug, Default)]
struct PrintfSpec {
    left: bool,
    plus: bool,
    space: bool,
    zero: bool,
    alternate: bool,
    width: usize,
    precision: Option<usize>,
    conversion: u8,
}

fn parse_printf_spec(format: &[u8], index: &mut usize) -> Result<PrintfSpec, ()> {
    let mut spec = PrintfSpec::default();
    while *index < format.len() {
        match format[*index] {
            b'-' => spec.left = true,
            b'+' => spec.plus = true,
            b' ' => spec.space = true,
            b'0' => spec.zero = true,
            b'#' => spec.alternate = true,
            _ => break,
        }
        *index += 1;
    }
    while *index < format.len() && format[*index].is_ascii_digit() {
        spec.width = spec
            .width
            .saturating_mul(10)
            .saturating_add(usize::from(format[*index] - b'0'))
            .min(MAX_COMMAND_OUTPUT_BYTES);
        *index += 1;
    }
    if format.get(*index) == Some(&b'.') {
        *index += 1;
        let mut precision = 0usize;
        while *index < format.len() && format[*index].is_ascii_digit() {
            precision = precision
                .saturating_mul(10)
                .saturating_add(usize::from(format[*index] - b'0'))
                .min(MAX_COMMAND_OUTPUT_BYTES);
            *index += 1;
        }
        spec.precision = Some(precision);
    }
    let conversion = *format.get(*index).ok_or(())?;
    *index += 1;
    if !matches!(
        conversion,
        b'%' | b's' | b'b' | b'c' | b'd' | b'i' | b'u' | b'o' | b'x' | b'X'
    ) {
        return Err(());
    }
    spec.conversion = conversion;
    Ok(spec)
}

fn render_printf_conversion(
    spec: &PrintfSpec,
    argument: &[u8],
    output: &mut OutputBuilder,
) -> Result<bool, ()> {
    match spec.conversion {
        b's' => {
            let data = string_prefix(argument, spec.precision);
            let display_len = std::str::from_utf8(data)
                .map(|text| text.chars().count())
                .unwrap_or(data.len());
            if !emit_padded_with_len(output, data, display_len, spec.width, spec.left, false) {
                return Ok(true);
            }
            Ok(false)
        }
        b'b' => {
            let (data, stopped) = decode_escapes(argument);
            let data = spec
                .precision
                .map_or(data.as_slice(), |limit| &data[..data.len().min(limit)]);
            if !emit_padded(output, data, spec.width, spec.left, false) {
                return Ok(true);
            }
            Ok(stopped)
        }
        b'c' => {
            let data = if argument.is_empty() {
                b"\0".as_slice()
            } else if let Some(length) = argument.char_len() {
                &argument[..length]
            } else {
                argument.get(..1).unwrap_or_default()
            };
            let display_len = std::str::from_utf8(data)
                .map(|text| text.chars().count())
                .unwrap_or(data.len());
            Ok(!emit_padded_with_len(
                output,
                data,
                display_len,
                spec.width,
                spec.left,
                false,
            ))
        }
        b'd' | b'i' => match parse_signed(argument, spec.conversion) {
            Ok(value) => Ok(!emit_integer(output, *spec, value as u64, true)),
            Err(()) => {
                output.stderr(b"printf: invalid number\n");
                Ok(!emit_integer(output, *spec, 0, true))
            }
        },
        b'u' | b'o' | b'x' | b'X' => match parse_unsigned(argument, spec.conversion) {
            Ok(value) => Ok(!emit_integer(output, *spec, value, false)),
            Err(()) => {
                output.stderr(b"printf: invalid number\n");
                Ok(!emit_integer(output, *spec, 0, false))
            }
        },
        b'%' => Ok(!output.stdout(b"%")),
        _ => Err(()),
    }
}

// `str::char_indices` cannot be used to slice invalid bytes, but command
// arguments are validated UTF-8. This helper returns the byte length of the
// first scalar without converting the whole argument into a lossy string.
trait FirstCharBytes {
    fn char_len(&self) -> Option<usize>;
}

impl FirstCharBytes for [u8] {
    fn char_len(&self) -> Option<usize> {
        std::str::from_utf8(self)
            .ok()?
            .chars()
            .next()
            .map(char::len_utf8)
    }
}

fn string_prefix(argument: &[u8], precision: Option<usize>) -> &[u8] {
    let Some(limit) = precision else {
        return argument;
    };
    let Ok(text) = std::str::from_utf8(argument) else {
        return &argument[..argument.len().min(limit)];
    };
    text.char_indices()
        .nth(limit)
        .map_or(argument, |(index, _)| &argument[..index])
}

fn emit_padded(
    output: &mut OutputBuilder,
    data: &[u8],
    width: usize,
    left: bool,
    zero: bool,
) -> bool {
    emit_padded_with_len(output, data, data.len(), width, left, zero)
}

fn emit_padded_with_len(
    output: &mut OutputBuilder,
    data: &[u8],
    data_len: usize,
    width: usize,
    left: bool,
    zero: bool,
) -> bool {
    let padding = width.saturating_sub(data_len);
    if !left && !write_repeated(output, if zero { b'0' } else { b' ' }, padding) {
        return false;
    }
    if !output.stdout(data) {
        return false;
    }
    if left {
        write_repeated(output, b' ', padding)
    } else {
        true
    }
}

fn write_repeated(output: &mut OutputBuilder, byte: u8, mut count: usize) -> bool {
    let block = [byte; 64];
    while count > 0 {
        let amount = count.min(block.len());
        if !output.stdout(&block[..amount]) {
            return false;
        }
        count -= amount;
    }
    true
}

fn is_valid_printf_number(spec: &PrintfSpec, argument: &[u8]) -> bool {
    match spec.conversion {
        b'd' | b'i' => parse_signed(argument, spec.conversion).is_ok(),
        b'u' | b'o' | b'x' | b'X' => parse_unsigned(argument, spec.conversion).is_ok(),
        _ => true,
    }
}

fn parse_signed(argument: &[u8], _conversion: u8) -> Result<i64, ()> {
    let text = std::str::from_utf8(argument).map_err(|_| ())?;
    if text.is_empty() {
        return Ok(0);
    }
    if matches!(text.as_bytes().first(), Some(b'\'' | b'"')) {
        return Ok(text.chars().nth(1).map_or(0, |value| value as i64));
    }
    let (negative, unsigned_text) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if unsigned_text.is_empty() {
        return Err(());
    }
    let (radix, digits) = if let Some(rest) = unsigned_text
        .strip_prefix("0x")
        .or_else(|| unsigned_text.strip_prefix("0X"))
    {
        (16, rest)
    } else if unsigned_text.len() > 1 && unsigned_text.starts_with('0') {
        (8, unsigned_text)
    } else {
        (10, unsigned_text)
    };
    let magnitude = i128::from_str_radix(digits, radix).map_err(|_| ())?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| ())
}

fn parse_unsigned(argument: &[u8], _conversion: u8) -> Result<u64, ()> {
    let text = std::str::from_utf8(argument).map_err(|_| ())?;
    if text.is_empty() {
        return Ok(0);
    }
    if matches!(text.as_bytes().first(), Some(b'\'' | b'"')) {
        return Ok(text.chars().nth(1).map_or(0, |value| value as u64));
    }
    let (negative, unsigned_text) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if unsigned_text.is_empty() {
        return Err(());
    }
    let (radix, digits) = if let Some(rest) = unsigned_text
        .strip_prefix("0x")
        .or_else(|| unsigned_text.strip_prefix("0X"))
    {
        (16, rest)
    } else if unsigned_text.len() > 1 && unsigned_text.starts_with('0') {
        (8, unsigned_text)
    } else {
        (10, unsigned_text)
    };
    if digits.is_empty() {
        return Err(());
    }
    let value = u64::from_str_radix(digits, radix).map_err(|_| ())?;
    Ok(if negative {
        0u64.wrapping_sub(value)
    } else {
        value
    })
}

fn emit_integer(output: &mut OutputBuilder, spec: PrintfSpec, value: u64, signed: bool) -> bool {
    let negative = signed && (value as i64) < 0;
    let magnitude = if negative {
        (0u64).wrapping_sub(value)
    } else {
        value
    };
    let radix = match spec.conversion {
        b'o' => 8,
        b'x' | b'X' => 16,
        _ => 10,
    };
    let mut digits = integer_digits(magnitude, radix, spec.precision, spec.conversion == b'X');
    if let Some(0) = spec.precision {
        if magnitude == 0 {
            digits.clear();
        }
    }

    let mut prefix = Vec::new();
    if spec.alternate {
        match spec.conversion {
            b'o' => {
                if !digits.starts_with(b"0") {
                    prefix.push(b'0');
                }
            }
            b'x' if magnitude != 0 => prefix.extend_from_slice(b"0x"),
            b'X' if magnitude != 0 => prefix.extend_from_slice(b"0X"),
            _ => {}
        }
    }
    let sign = if signed {
        if negative {
            Some(b'-')
        } else if spec.plus {
            Some(b'+')
        } else if spec.space {
            Some(b' ')
        } else {
            None
        }
    } else {
        None
    };
    let total_len = usize::from(sign.is_some())
        .saturating_add(prefix.len())
        .saturating_add(digits.len());
    let padding = spec.width.saturating_sub(total_len);
    if !spec.left
        && (!spec.zero || spec.precision.is_some())
        && !write_repeated(output, b' ', padding)
    {
        return false;
    }
    if let Some(sign) = sign {
        if !output.stdout(&[sign]) {
            return false;
        }
    }
    if !prefix.is_empty() && !output.stdout(&prefix) {
        return false;
    }
    if !spec.left && spec.zero && spec.precision.is_none() && !write_repeated(output, b'0', padding)
    {
        return false;
    }
    if !output.stdout(&digits) {
        return false;
    }
    if spec.left {
        write_repeated(output, b' ', padding)
    } else {
        true
    }
}

fn integer_digits(value: u64, radix: u32, precision: Option<usize>, uppercase: bool) -> Vec<u8> {
    let mut digits = Vec::new();
    let alphabet = if radix == 16 {
        if uppercase {
            b"0123456789ABCDEF".as_slice()
        } else {
            b"0123456789abcdef".as_slice()
        }
    } else {
        b"01234567".as_slice()
    };
    let mut value = value;
    if value == 0 {
        digits.push(b'0');
    } else {
        while value > 0 {
            digits.push(alphabet[(value % u64::from(radix)) as usize]);
            value /= u64::from(radix);
        }
        digits.reverse();
    }
    let minimum = precision.unwrap_or(0);
    if digits.len() < minimum {
        let mut padded = vec![b'0'; minimum - digits.len()];
        padded.extend_from_slice(&digits);
        padded
    } else {
        digits
    }
}

fn decode_escapes(input: &[u8]) -> (Vec<u8>, bool) {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    let mut stopped = false;
    while index < input.len() {
        if input[index] != b'\\' {
            output.push(input[index]);
            index += 1;
            continue;
        }
        index += 1;
        let (bytes, stop) = decode_escape_at(input, &mut index);
        output.extend_from_slice(&bytes);
        if stop {
            stopped = true;
            break;
        }
    }
    (output, stopped)
}

fn decode_escape_at(input: &[u8], index: &mut usize) -> (Vec<u8>, bool) {
    if *index >= input.len() {
        return (vec![b'\\'], false);
    }
    let escaped = input[*index];
    *index += 1;
    match escaped {
        b'\\' => (vec![b'\\'], false),
        b'"' => (vec![b'"'], false),
        b'a' => (vec![0x07], false),
        b'b' => (vec![0x08], false),
        b'c' => (Vec::new(), true),
        b'e' | b'E' => (vec![0x1b], false),
        b'f' => (vec![0x0c], false),
        b'n' => (vec![b'\n'], false),
        b'r' => (vec![b'\r'], false),
        b't' => (vec![b'\t'], false),
        b'v' => (vec![0x0b], false),
        b'0'..=b'7' => {
            let max_follow = 2;
            let mut value = u16::from(escaped - b'0');
            let mut digits = 0;
            while *index < input.len() && digits < max_follow {
                let byte = input[*index];
                if !(b'0'..=b'7').contains(&byte) {
                    break;
                }
                value = value
                    .saturating_mul(8)
                    .saturating_add(u16::from(byte - b'0'));
                *index += 1;
                digits += 1;
            }
            (vec![(value & 0xff) as u8], false)
        }
        b'x' => {
            let start = *index;
            let mut value = 0u8;
            while *index < input.len() && *index - start < 2 {
                let Some(digit) = hex_value(input[*index]) else {
                    break;
                };
                value = value.saturating_mul(16).saturating_add(digit);
                *index += 1;
            }
            if *index == start {
                (b"\\x".to_vec(), false)
            } else {
                (vec![value], false)
            }
        }
        b'u' | b'U' => {
            let width = if escaped == b'u' { 4 } else { 8 };
            let start = *index;
            if input.len().saturating_sub(start) < width
                || !(0..width).all(|offset| hex_value(input[start + offset]).is_some())
            {
                let literal = vec![b'\\', escaped];
                return (literal, false);
            }
            let mut value = 0u32;
            for offset in 0..width {
                value = value * 16 + u32::from(hex_value(input[start + offset]).unwrap_or(0));
            }
            *index += width;
            if let Some(character) = char::from_u32(value) {
                let mut encoded = [0; 4];
                (
                    character.encode_utf8(&mut encoded).as_bytes().to_vec(),
                    false,
                )
            } else {
                let mut literal = vec![b'\\', escaped];
                literal.extend_from_slice(&input[start..start + width]);
                (literal, false)
            }
        }
        other => (vec![b'\\', other], false),
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextMode {
    Bytes(u64),
    Lines(u64),
}

#[derive(Clone, Debug)]
struct TextOptions {
    mode: TextMode,
    quiet: bool,
    verbose: bool,
    zero_terminated: bool,
    operands: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextParseError {
    InvalidNumber,
    InvalidOption,
    FollowUnavailable,
}

fn parse_text_options(args: &[String], is_tail: bool) -> Result<TextOptions, TextParseError> {
    let mut mode = TextMode::Lines(10);
    let mut quiet = false;
    let mut verbose = false;
    let mut zero_terminated = false;
    let mut operands = Vec::new();
    let mut options = true;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if options && arg == "--" {
            options = false;
            index += 1;
            continue;
        }
        if options && arg.starts_with('-') && arg != "-" {
            if is_tail
                && (matches!(
                    arg.as_str(),
                    "-f" | "-F"
                        | "--follow"
                        | "--retry"
                        | "--pid"
                        | "--sleep-interval"
                        | "--max-unchanged-stats"
                ) || [
                    "--follow=",
                    "--retry=",
                    "--pid=",
                    "--sleep-interval=",
                    "--max-unchanged-stats=",
                ]
                .iter()
                .any(|prefix| arg.starts_with(prefix)))
            {
                return Err(TextParseError::FollowUnavailable);
            }
            if let Some(value) = arg.strip_prefix("--bytes=") {
                mode = TextMode::Bytes(parse_count(value)?);
                index += 1;
                continue;
            }
            if let Some(value) = arg.strip_prefix("--lines=") {
                mode = TextMode::Lines(parse_count(value)?);
                index += 1;
                continue;
            }
            if arg == "-c" || arg == "-n" {
                let value = args.get(index + 1).ok_or(TextParseError::InvalidNumber)?;
                mode = if arg == "-c" {
                    TextMode::Bytes(parse_count(value)?)
                } else {
                    TextMode::Lines(parse_count(value)?)
                };
                index += 2;
                continue;
            }
            if arg == "-q" || arg == "--quiet" || arg == "--silent" {
                quiet = true;
                index += 1;
                continue;
            }
            if arg == "-v" || arg == "--verbose" {
                verbose = true;
                index += 1;
                continue;
            }
            if arg == "-z" || arg == "--zero-terminated" {
                zero_terminated = true;
                index += 1;
                continue;
            }
            return Err(TextParseError::InvalidOption);
        }
        operands.push(arg.clone());
        index += 1;
    }
    Ok(TextOptions {
        mode,
        quiet,
        verbose,
        zero_terminated,
        operands,
    })
}

fn parse_count(value: &str) -> Result<u64, TextParseError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(TextParseError::InvalidNumber);
    }
    value
        .parse::<u64>()
        .map_err(|_| TextParseError::InvalidNumber)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HeadCommand;

impl Command for HeadCommand {
    fn name(&self) -> &str {
        "head"
    }

    fn summary(&self) -> Option<&str> {
        Some("select the beginning of virtual byte or record streams")
    }

    fn run(&self, invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
        run_head_tail(invocation, backend, false)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TailCommand;

impl Command for TailCommand {
    fn name(&self) -> &str {
        "tail"
    }

    fn summary(&self) -> Option<&str> {
        Some("select the end of virtual byte or record streams")
    }

    fn run(&self, invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
        run_head_tail(invocation, backend, true)
    }
}

fn run_head_tail(
    invocation: &CommandInvocation,
    backend: &dyn WorkspaceBackend,
    is_tail: bool,
) -> CommandOutput {
    let command = if is_tail { "tail" } else { "head" };
    let options = match parse_text_options(invocation.args(), is_tail) {
        Ok(options) => options,
        Err(error) => {
            let diagnostic = match error {
                TextParseError::InvalidNumber => {
                    if is_tail {
                        b"tail: invalid number\n".as_slice()
                    } else {
                        b"head: invalid number\n".as_slice()
                    }
                }
                TextParseError::FollowUnavailable => b"tail: follow mode is unavailable\n",
                TextParseError::InvalidOption => {
                    if is_tail {
                        b"tail: invalid option\n".as_slice()
                    } else {
                        b"head: invalid option\n".as_slice()
                    }
                }
            };
            return failure_with_limits(2, diagnostic, invocation.limits());
        }
    };
    let operands = if options.operands.is_empty() {
        vec!["-".to_owned()]
    } else {
        options.operands
    };
    let show_headers = options.verbose || (!options.quiet && operands.len() > 1);
    let delimiter = if options.zero_terminated { 0 } else { b'\n' };
    let mut output = OutputBuilder::new(invocation.limits());
    let mut status = 0;
    let mut stdin_used = false;
    let zero_count = matches!(options.mode, TextMode::Bytes(0) | TextMode::Lines(0));

    for (operand_index, operand) in operands.iter().enumerate() {
        let path = if operand == "-" {
            None
        } else {
            match resolve_virtual_path(invocation.cwd(), operand) {
                Ok(path) => Some(path),
                Err(_) => {
                    output.stderr(format!("{command}: invalid virtual path\n").as_bytes());
                    status = 2;
                    continue;
                }
            }
        };
        if show_headers {
            if operand_index > 0 && !output.stdout(b"\n") {
                break;
            }
            let label = path
                .as_ref()
                .map_or("standard input", |value| value.as_str());
            if !output.stdout(format!("==> {label} <==\n").as_bytes()) {
                break;
            }
        }
        if zero_count {
            continue;
        }
        if operand == "-" {
            if stdin_used {
                continue;
            }
            stdin_used = true;
            let Some(stdin) = invocation.stdin() else {
                output.stderr(format!("{command}: stdin: Bad file descriptor\n").as_bytes());
                status = 1;
                continue;
            };
            let result = if is_tail {
                render_tail_slice(stdin, options.mode, delimiter, &mut output)
            } else {
                render_head_slice(stdin, options.mode, delimiter, &mut output)
            };
            if result.is_err() {
                status = 1;
            }
        } else {
            let Some(path) = path.as_ref() else { continue };
            let info = match backend.stat(path) {
                Ok(info) if info.kind == EntryKind::File => info,
                Ok(_) | Err(_) => {
                    output.stderr(format!("{command}: cannot read file\n").as_bytes());
                    status = 1;
                    continue;
                }
            };
            let result = if is_tail {
                render_tail_file(
                    backend,
                    path,
                    info.size,
                    options.mode,
                    delimiter,
                    &mut output,
                )
            } else {
                render_head_file(
                    backend,
                    path,
                    info.size,
                    options.mode,
                    delimiter,
                    &mut output,
                )
            };
            if result.is_err() {
                output.stderr(format!("{command}: cannot read file\n").as_bytes());
                status = 1;
            }
        }
        if output.exceeded {
            status = 1;
            break;
        }
    }
    output.finish(status)
}

fn render_head_slice(
    data: &[u8],
    mode: TextMode,
    delimiter: u8,
    output: &mut OutputBuilder,
) -> Result<(), ()> {
    match mode {
        TextMode::Bytes(count) => {
            let length = min(count, data.len() as u64) as usize;
            if output.stdout(&data[..length]) {
                Ok(())
            } else {
                Err(())
            }
        }
        TextMode::Lines(count) => {
            if count == 0 {
                return Ok(());
            }
            let mut records = 0u64;
            let mut start = 0usize;
            for (index, byte) in data.iter().copied().enumerate() {
                if byte == delimiter {
                    if !output.stdout(&data[start..=index]) {
                        return Err(());
                    }
                    records = records.saturating_add(1);
                    start = index + 1;
                    if records >= count {
                        return Ok(());
                    }
                }
            }
            if start < data.len() && !output.stdout(&data[start..]) {
                return Err(());
            }
            Ok(())
        }
    }
}

fn render_head_file(
    backend: &dyn WorkspaceBackend,
    path: &VirtualPath,
    size: u64,
    mode: TextMode,
    delimiter: u8,
    output: &mut OutputBuilder,
) -> Result<(), WorkspaceError> {
    match mode {
        TextMode::Bytes(count) => {
            let target = size.min(count);
            let mut scan_budget = MAX_COMMAND_SCAN_BYTES;
            stream_file_range(backend, path, 0, target, &mut scan_budget, &mut |part| {
                output.stdout(part)
            })
            .map(|_| ())
        }
        TextMode::Lines(count) => {
            if count == 0 {
                return Ok(());
            }
            let mut records = 0u64;
            let mut scan_budget = MAX_COMMAND_SCAN_BYTES;
            stream_file_range(backend, path, 0, size, &mut scan_budget, &mut |part| {
                let mut start = 0;
                for (index, byte) in part.iter().copied().enumerate() {
                    if byte == delimiter {
                        if !output.stdout(&part[start..=index]) {
                            return false;
                        }
                        records = records.saturating_add(1);
                        start = index + 1;
                        if records >= count {
                            return false;
                        }
                    }
                }
                if records < count && start < part.len() && !output.stdout(&part[start..]) {
                    return false;
                }
                true
            })
            .map(|_| ())
        }
    }
}

fn render_tail_slice(
    data: &[u8],
    mode: TextMode,
    delimiter: u8,
    output: &mut OutputBuilder,
) -> Result<(), ()> {
    match mode {
        TextMode::Bytes(count) => {
            let start = data
                .len()
                .saturating_sub(count.min(usize::MAX as u64) as usize);
            if output.stdout(&data[start..]) {
                Ok(())
            } else {
                Err(())
            }
        }
        TextMode::Lines(count) => {
            let start = tail_line_start(data, count, delimiter);
            if output.stdout(&data[start..]) {
                Ok(())
            } else {
                Err(())
            }
        }
    }
}

fn render_tail_file(
    backend: &dyn WorkspaceBackend,
    path: &VirtualPath,
    size: u64,
    mode: TextMode,
    delimiter: u8,
    output: &mut OutputBuilder,
) -> Result<(), WorkspaceError> {
    if matches!(mode, TextMode::Bytes(0) | TextMode::Lines(0)) {
        return Ok(());
    }
    let mut scan_budget = MAX_COMMAND_SCAN_BYTES;
    let start = match mode {
        TextMode::Bytes(count) => size.saturating_sub(count),
        TextMode::Lines(count) => {
            find_tail_line_start(backend, path, size, count, delimiter, &mut scan_budget)?
        }
    };
    stream_file_range(
        backend,
        path,
        start,
        size.saturating_sub(start),
        &mut scan_budget,
        &mut |part| output.stdout(part),
    )
    .map(|_| ())
}

fn tail_line_start(data: &[u8], count: u64, delimiter: u8) -> usize {
    if count == 0 || data.is_empty() {
        return data.len();
    }
    let mut seen = 0u64;
    let mut skip_trailing = data.last() == Some(&delimiter);
    for index in (0..data.len()).rev() {
        if skip_trailing && index + 1 == data.len() && data[index] == delimiter {
            skip_trailing = false;
            continue;
        }
        skip_trailing = false;
        if data[index] == delimiter {
            seen = seen.saturating_add(1);
            if seen >= count {
                return index + 1;
            }
        }
    }
    0
}

fn find_tail_line_start(
    backend: &dyn WorkspaceBackend,
    path: &VirtualPath,
    size: u64,
    count: u64,
    delimiter: u8,
    scan_budget: &mut u64,
) -> Result<u64, WorkspaceError> {
    if count == 0 || size == 0 {
        return Ok(size);
    }
    let mut cursor = size;
    let mut seen = 0u64;
    let mut skip_trailing = true;
    let chunk = backend_read_chunk(backend)?;
    while cursor > 0 {
        let offset = cursor.saturating_sub(chunk as u64);
        let requested = cursor - offset;
        if requested > *scan_budget {
            return Err(WorkspaceError::Limit(msp_backend::LimitError::bounded(
                msp_backend::LimitKind::ReadBytes,
                MAX_COMMAND_SCAN_BYTES,
            )));
        }
        *scan_budget -= requested;
        let part = backend.read_range(path, ByteRange::new(offset, requested))?;
        if part.is_empty() {
            return Err(WorkspaceError::NotFound);
        }
        for index in (0..part.len()).rev() {
            let position = offset.saturating_add(index as u64);
            if skip_trailing && position.saturating_add(1) == size && part[index] == delimiter {
                skip_trailing = false;
                continue;
            }
            skip_trailing = false;
            if part[index] == delimiter {
                seen = seen.saturating_add(1);
                if seen >= count {
                    return Ok(position.saturating_add(1));
                }
            }
        }
        if (part.len() as u64) < requested {
            break;
        }
        cursor = offset;
    }
    Ok(0)
}

fn stream_file_range(
    backend: &dyn WorkspaceBackend,
    path: &VirtualPath,
    offset: u64,
    length: u64,
    scan_budget: &mut u64,
    callback: &mut impl FnMut(&[u8]) -> bool,
) -> Result<bool, WorkspaceError> {
    let chunk = backend_read_chunk(backend)? as u64;
    let end = offset.saturating_add(length);
    let mut cursor = offset;
    while cursor < end {
        if *scan_budget == 0 {
            return Err(WorkspaceError::Limit(msp_backend::LimitError::bounded(
                msp_backend::LimitKind::ReadBytes,
                MAX_COMMAND_SCAN_BYTES,
            )));
        }
        let requested = chunk.min(end - cursor).min(*scan_budget);
        let part = backend.read_range(path, ByteRange::new(cursor, requested))?;
        if part.is_empty() {
            return Err(WorkspaceError::NotFound);
        }
        let part_len = part.len() as u64;
        if part_len > requested {
            return Err(WorkspaceError::Limit(msp_backend::LimitError::bounded(
                msp_backend::LimitKind::ReadBytes,
                requested,
            )));
        }
        cursor = cursor.saturating_add(part_len);
        *scan_budget = scan_budget.saturating_sub(part_len);
        if !callback(&part) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn backend_read_chunk(backend: &dyn WorkspaceBackend) -> Result<usize, WorkspaceError> {
    let limit = usize::try_from(backend.limits().max_read_bytes).unwrap_or(usize::MAX);
    let chunk = FILE_READ_CHUNK_BYTES.min(limit);
    if chunk == 0 {
        Err(WorkspaceError::Limit(msp_backend::LimitError::bounded(
            msp_backend::LimitKind::ReadBytes,
            backend.limits().max_read_bytes,
        )))
    } else {
        Ok(chunk)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WcCommand;

impl Command for WcCommand {
    fn name(&self) -> &str {
        "wc"
    }

    fn summary(&self) -> Option<&str> {
        Some("count lines, ASCII words, and bytes in virtual input")
    }

    fn run(&self, invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
        run_wc(invocation, backend)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct WcOptions {
    lines: bool,
    words: bool,
    bytes: bool,
    explicit_selection: bool,
}

fn run_wc(invocation: &CommandInvocation, backend: &dyn WorkspaceBackend) -> CommandOutput {
    let (options, operands) = match parse_wc_options(invocation.args()) {
        Ok(value) => value,
        Err(()) => return failure_with_limits(2, b"wc: invalid option\n", invocation.limits()),
    };
    let operands = if operands.is_empty() {
        vec![None]
    } else {
        operands.into_iter().map(Some).collect::<Vec<_>>()
    };
    let multiple = operands.len() > 1;
    let mut output = OutputBuilder::new(invocation.limits());
    let mut status = 0;
    let mut stdin_used = false;
    let mut total = WcCounts::default();

    for operand in operands {
        let mut counts = WcCounts::default();
        let mut label = None;
        let result = match operand.as_deref() {
            None => {
                let Some(stdin) = invocation.stdin() else {
                    output.stderr(b"wc: stdin: Bad file descriptor\n");
                    status = 1;
                    continue;
                };
                count_slice(stdin, &mut counts);
                Ok(())
            }
            Some("-") => {
                label = Some("-".to_owned());
                if stdin_used {
                    Ok(())
                } else {
                    stdin_used = true;
                    let Some(stdin) = invocation.stdin() else {
                        output.stderr(b"wc: stdin: Bad file descriptor\n");
                        status = 1;
                        continue;
                    };
                    count_slice(stdin, &mut counts);
                    Ok(())
                }
            }
            Some(raw) => {
                let path = match resolve_virtual_path(invocation.cwd(), raw) {
                    Ok(path) => path,
                    Err(_) => {
                        output.stderr(b"wc: invalid virtual path\n");
                        status = 2;
                        continue;
                    }
                };
                label = Some(path.to_string());
                match backend.stat(&path) {
                    Ok(info) if info.kind == EntryKind::File => {
                        count_file(backend, &path, info.size, &mut counts)
                    }
                    Ok(_) | Err(_) => {
                        output.stderr(b"wc: cannot read file\n");
                        status = 1;
                        continue;
                    }
                }
            }
        };
        if result.is_err() {
            output.stderr(b"wc: cannot read file\n");
            status = 1;
            continue;
        }
        total.add(counts);
        let row = format_wc_row(&options, &counts, label.as_deref());
        if !output.stdout(&row) {
            status = 1;
            break;
        }
    }
    if multiple && !output.exceeded {
        let row = format_wc_row(&options, &total, Some("total"));
        if !output.stdout(&row) {
            status = 1;
        }
    }
    output.finish(status)
}

fn parse_wc_options(args: &[String]) -> Result<(WcOptions, Vec<String>), ()> {
    let mut options = WcOptions::default();
    let mut operands = Vec::new();
    let mut parse_options = true;
    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.starts_with('-') && arg != "-" {
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--lines" => options.lines = true,
                    "--words" => options.words = true,
                    "--bytes" => options.bytes = true,
                    _ => return Err(()),
                }
                options.explicit_selection = true;
                continue;
            }
            match arg.as_str() {
                "-l" => options.lines = true,
                "-w" => options.words = true,
                "-c" => options.bytes = true,
                _ => return Err(()),
            }
            options.explicit_selection = true;
            continue;
        }
        operands.push(arg.clone());
    }
    if !options.explicit_selection {
        options.lines = true;
        options.words = true;
        options.bytes = true;
    }
    Ok((options, operands))
}

#[derive(Clone, Copy, Debug, Default)]
struct WcCounts {
    lines: u64,
    words: u64,
    bytes: u64,
}

impl WcCounts {
    fn add(&mut self, other: Self) {
        self.lines = self.lines.saturating_add(other.lines);
        self.words = self.words.saturating_add(other.words);
        self.bytes = self.bytes.saturating_add(other.bytes);
    }
}

fn count_slice(data: &[u8], counts: &mut WcCounts) {
    let mut in_word = false;
    for &byte in data {
        counts.bytes = counts.bytes.saturating_add(1);
        if byte == b'\n' {
            counts.lines = counts.lines.saturating_add(1);
        }
        if matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
            in_word = false;
        } else if !in_word {
            counts.words = counts.words.saturating_add(1);
            in_word = true;
        }
    }
}

fn count_file(
    backend: &dyn WorkspaceBackend,
    path: &VirtualPath,
    size: u64,
    counts: &mut WcCounts,
) -> Result<(), WorkspaceError> {
    let mut in_word = false;
    let mut scan_budget = MAX_COMMAND_SCAN_BYTES;
    stream_file_range(backend, path, 0, size, &mut scan_budget, &mut |part| {
        for &byte in part {
            counts.bytes = counts.bytes.saturating_add(1);
            if byte == b'\n' {
                counts.lines = counts.lines.saturating_add(1);
            }
            if matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
                in_word = false;
            } else if !in_word {
                counts.words = counts.words.saturating_add(1);
                in_word = true;
            }
        }
        true
    })
    .map(|_| ())
}

fn format_wc_row(options: &WcOptions, counts: &WcCounts, label: Option<&str>) -> Vec<u8> {
    let mut fields = Vec::new();
    if options.lines {
        fields.push(counts.lines.to_string());
    }
    if options.words {
        fields.push(counts.words.to_string());
    }
    if options.bytes {
        fields.push(counts.bytes.to_string());
    }
    let mut row = fields.join(" ");
    if let Some(label) = label {
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(label);
    }
    row.push('\n');
    row.into_bytes()
}

pub(crate) fn command_values() -> Vec<Box<dyn Command>> {
    vec![
        Box::new(HeadCommand),
        Box::new(PrintfCommand),
        Box::new(TailCommand),
        Box::new(WcCommand),
    ]
}
