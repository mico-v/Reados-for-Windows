using System.Runtime.InteropServices;
using System.Text;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class WindowsMspNativeTransportTests
{
    [Fact]
    public void Abi_v2_contract_uses_the_frozen_fixed_width_layout()
    {
        Assert.Equal(32, Marshal.SizeOf<MspAbiInfoV2>());
        Assert.Equal(0, Marshal.OffsetOf<MspAbiInfoV2>(nameof(MspAbiInfoV2.Size)).ToInt32());
        Assert.Equal(4, Marshal.OffsetOf<MspAbiInfoV2>(nameof(MspAbiInfoV2.MajorVersion)).ToInt32());
        Assert.Equal(8, Marshal.OffsetOf<MspAbiInfoV2>(nameof(MspAbiInfoV2.MinorVersion)).ToInt32());
        Assert.Equal(12, Marshal.OffsetOf<MspAbiInfoV2>(nameof(MspAbiInfoV2.Reserved)).ToInt32());
        Assert.Equal(16, Marshal.OffsetOf<MspAbiInfoV2>(nameof(MspAbiInfoV2.ContractId)).ToInt32());
        Assert.Equal(24, Marshal.OffsetOf<MspAbiInfoV2>(nameof(MspAbiInfoV2.Capabilities)).ToInt32());
        Assert.Equal(2U, MspNativeContract.AbiV2MajorVersion);
        Assert.Equal(0U, MspNativeContract.AbiV2MinorVersion);
        Assert.Equal(0x324D534F44414552UL, MspNativeContract.AbiV2ContractId);
        Assert.Equal(0xFUL, MspNativeContract.AbiV2RequiredCapabilities);
    }

    [Fact]
    public void Abi_v2_workspace_read_capability_is_the_fourth_optional_bit()
    {
        Assert.Equal(1UL << 4, (ulong)MspNativeAbiV2Capabilities.WorkspaceRead);
        Assert.Equal(
            0x1FUL,
            (ulong)MspNativeAbiV2Capabilities.LengthDelimitedJson |
            (ulong)MspNativeAbiV2Capabilities.Execute |
            (ulong)MspNativeAbiV2Capabilities.Parse |
            (ulong)MspNativeAbiV2Capabilities.Normalize |
            (ulong)MspNativeAbiV2Capabilities.WorkspaceRead);
    }

    [Fact]
    public void Constructor_rejects_missing_library_without_disclosing_path()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var secretPath = Path.Combine(
            Path.GetTempPath(),
            "private-secret-location",
            "msp_core.dll");
        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            new WindowsMspNativeTransport(
                Options(secretPath),
                new FakeLibraryLoader(null)));

        Assert.Equal(MspNativeFailureKind.LibraryUnavailable, exception.FailureKind);
        Assert.False(exception.ToString().Contains(secretPath, StringComparison.OrdinalIgnoreCase));
        Assert.DoesNotContain("WindowsMspNativeTransport.cs", exception.ToString());
        Assert.DoesNotContain("StackTrace", exception.ToString());
        Assert.Contains("failure=LibraryUnavailable", exception.ToString());
        Assert.Null(exception.InnerException);
    }

    [Fact]
    public void Constructor_sanitizes_library_loader_failure()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        const string secret = @"V:\private\loader-secret\msp_core.dll";
        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            new WindowsMspNativeTransport(
                Options(AbsoluteLibraryPath()),
                new ThrowingLibraryLoader(new InvalidOperationException(secret))));

        Assert.Equal(MspNativeFailureKind.LibraryUnavailable, exception.FailureKind);
        Assert.DoesNotContain(secret, exception.ToString());
        Assert.Null(exception.InnerException);
    }

    [Fact]
    public void Constructor_rejects_missing_export_and_releases_library()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var library = new FakeNativeLibrary();
        library.Add(MspNativeContract.ExecuteExport, new MspNativeJsonFunction(_ => nint.Zero));
        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            new WindowsMspNativeTransport(
                Options(AbsoluteLibraryPath()),
                new FakeLibraryLoader(library)));

        Assert.Equal(MspNativeFailureKind.ExportUnavailable, exception.FailureKind);
        Assert.True(library.IsDisposed);
    }

    [Theory]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    [InlineData(4)]
    [InlineData(5)]
    [InlineData(6)]
    public void Constructor_rejects_every_partial_v2_export_set_without_v1_fallback(
        int exportMask)
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var library = new FakeNativeLibrary();
        AddV1Exports(library, _ => nint.Zero, _ => { });
        AddSelectedV2Exports(library, exportMask);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            new WindowsMspNativeTransport(
                Options(AbsoluteLibraryPath()),
                new FakeLibraryLoader(library)));

        Assert.Equal(MspNativeFailureKind.AbiExportSetIncomplete, exception.FailureKind);
        Assert.True(library.IsDisposed);
        Assert.DoesNotContain(MspNativeContract.ExecuteExport, library.QueriedExports);
        Assert.DoesNotContain(MspNativeContract.ParseExport, library.QueriedExports);
        Assert.DoesNotContain(MspNativeContract.NormalizeWorkspacePathExport, library.QueriedExports);
        Assert.DoesNotContain(MspNativeContract.FreeStringExport, library.QueriedExports);
    }

    [Fact]
    public void Constructor_treats_reported_v2_export_with_null_address_as_malformed()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var library = new FakeNativeLibrary();
        library.AddAddress(MspNativeContract.GetAbiInfoV2Export, nint.Zero);
        AddV1Exports(library, _ => nint.Zero, _ => { });

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            new WindowsMspNativeTransport(
                Options(AbsoluteLibraryPath()),
                new FakeLibraryLoader(library)));

        Assert.Equal(MspNativeFailureKind.AbiExportSetIncomplete, exception.FailureKind);
        Assert.True(library.IsDisposed);
        Assert.DoesNotContain(MspNativeContract.ExecuteExport, library.QueriedExports);
    }

    [Theory]
    [InlineData("size-under", MspNativeFailureKind.AbiLayoutMismatch)]
    [InlineData("size-over", MspNativeFailureKind.AbiLayoutMismatch)]
    [InlineData("major", MspNativeFailureKind.AbiVersionMismatch)]
    [InlineData("minor", MspNativeFailureKind.AbiVersionMismatch)]
    [InlineData("reserved", MspNativeFailureKind.AbiReservedFieldInvalid)]
    [InlineData("contract", MspNativeFailureKind.AbiContractMismatch)]
    [InlineData("capabilities", MspNativeFailureKind.AbiCapabilitiesMissing)]
    public void Constructor_fails_closed_for_every_v2_handshake_mismatch(
        string mismatch,
        MspNativeFailureKind expectedFailure)
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var abiInfo = ValidAbiInfo();
        switch (mismatch)
        {
            case "size-under":
                abiInfo.Size--;
                break;
            case "size-over":
                abiInfo.Size++;
                break;
            case "major":
                abiInfo.MajorVersion++;
                break;
            case "minor":
                abiInfo.MinorVersion++;
                break;
            case "reserved":
                abiInfo.Reserved = 1;
                break;
            case "contract":
                abiInfo.ContractId++;
                break;
            case "capabilities":
                abiInfo.Capabilities &= ~1UL;
                break;
            default:
                throw new InvalidOperationException(mismatch);
        }

        var invokeCount = 0;
        var library = CreateV2Library(
            abiInfo,
            (uint _, nint _, ulong _, ref nint response, ref ulong responseLength) =>
            {
                invokeCount++;
                response = nint.Zero;
                responseLength = 0;
                return 0;
            },
            (_, _) => { },
            includeV1: true);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            new WindowsMspNativeTransport(
                Options(AbsoluteLibraryPath()),
                new FakeLibraryLoader(library)));

        Assert.Equal(expectedFailure, exception.FailureKind);
        Assert.Equal(0, invokeCount);
        Assert.True(library.IsDisposed);
        Assert.DoesNotContain(MspNativeContract.ExecuteExport, library.QueriedExports);
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    public void Constructor_rejects_each_missing_required_v2_capability(int bit)
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var abiInfo = ValidAbiInfo();
        abiInfo.Capabilities &= ~(1UL << bit);
        var library = CreateV2Library(abiInfo, DefaultV2Invoke, FreeV2, includeV1: true);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            new WindowsMspNativeTransport(
                Options(AbsoluteLibraryPath()),
                new FakeLibraryLoader(library)));

        Assert.Equal(MspNativeFailureKind.AbiCapabilitiesMissing, exception.FailureKind);
        Assert.True(library.IsDisposed);
        Assert.DoesNotContain(MspNativeContract.ExecuteExport, library.QueriedExports);
    }

    [Theory]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(5)]
    [InlineData(99)]
    public void Constructor_rejects_nonzero_or_unknown_v2_handshake_status(int status)
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var library = CreateV2Library(
            ValidAbiInfo(),
            DefaultV2Invoke,
            FreeV2,
            handshakeStatus: status,
            includeV1: true);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            new WindowsMspNativeTransport(
                Options(AbsoluteLibraryPath()),
                new FakeLibraryLoader(library)));

        Assert.Equal(MspNativeFailureKind.AbiHandshakeFailed, exception.FailureKind);
        Assert.True(library.IsDisposed);
        Assert.DoesNotContain(MspNativeContract.ExecuteExport, library.QueriedExports);
    }

    [Fact]
    public void Constructor_sanitizes_v2_handshake_exception_and_never_falls_back()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        const string secret = @"V:\private\abi-handshake-secret";
        var library = new FakeNativeLibrary();
        library.Add(
            MspNativeContract.GetAbiInfoV2Export,
            new MspNativeGetAbiInfoV2Function(
                (ref MspAbiInfoV2 _, uint _) => throw new InvalidOperationException(secret)));
        library.Add(
            MspNativeContract.InvokeV2Export,
            new MspNativeInvokeV2Function(DefaultV2Invoke));
        library.Add(
            MspNativeContract.FreeBufferV2Export,
            new MspNativeFreeBufferV2Function(FreeV2));
        AddV1Exports(library, _ => nint.Zero, _ => { });

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            new WindowsMspNativeTransport(
                Options(AbsoluteLibraryPath()),
                new FakeLibraryLoader(library)));

        Assert.Equal(MspNativeFailureKind.AbiHandshakeFailed, exception.FailureKind);
        Assert.DoesNotContain(secret, exception.ToString());
        Assert.DoesNotContain(MspNativeContract.ExecuteExport, library.QueriedExports);
        Assert.True(library.IsDisposed);
    }

    [Fact]
    public void Constructor_preserves_handshake_failure_when_library_disposal_also_fails()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        const string secret = @"V:\private\dispose-secret";
        var abiInfo = ValidAbiInfo();
        abiInfo.ContractId++;
        var library = CreateV2Library(abiInfo, DefaultV2Invoke, FreeV2);
        library.DisposeException = new InvalidOperationException(secret);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            new WindowsMspNativeTransport(
                Options(AbsoluteLibraryPath()),
                new FakeLibraryLoader(library)));

        Assert.Equal(MspNativeFailureKind.AbiContractMismatch, exception.FailureKind);
        Assert.DoesNotContain(secret, exception.ToString());
        Assert.Equal(1, library.DisposeCount);
    }

    [Fact]
    public void Constructor_uses_complete_v1_only_when_all_v2_exports_are_absent()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var v1FreeCount = 0;
        var library = CreateLibrary(
            _ => AllocateNullTerminated("{}"u8.ToArray()),
            pointer =>
            {
                v1FreeCount++;
                Marshal.FreeHGlobal(pointer);
            });

        using var transport = new WindowsMspNativeTransport(
            Options(AbsoluteLibraryPath()),
            new FakeLibraryLoader(library));

        Assert.Equal(MspNativeAbiMode.LegacyV1, transport.AbiMode);
        Assert.Equal(MspNativeAbiMode.LegacyV1, transport.NativeRuntimeInfo.AbiMode);
        Assert.Equal(0U, transport.NativeRuntimeInfo.MajorVersion);
        Assert.Equal("{}", Encoding.UTF8.GetString(
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray())));
        Assert.Equal(1, v1FreeCount);
    }

    [Fact]
    public void Constructor_prefers_complete_v2_and_exposes_negotiated_runtime_info()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var handshakeCount = 0;
        var invokeCount = 0;
        var v1InvokeCount = 0;
        var v1FreeCount = 0;
        var v2FreeCount = 0;
        var abiInfo = ValidAbiInfo();
        abiInfo.Capabilities |= 1UL << 17;
        var library = CreateV2Library(
            abiInfo,
            (uint operation, nint _, ulong _, ref nint response, ref ulong responseLength) =>
            {
                Assert.Equal(1U, operation);
                Assert.Equal(1, handshakeCount);
                invokeCount++;
                var bytes = "{}"u8.ToArray();
                response = Allocate(bytes);
                responseLength = (ulong)bytes.Length;
                return 0;
            },
            (pointer, _) =>
            {
                v2FreeCount++;
                Marshal.FreeHGlobal(pointer);
            },
            includeV1: true,
            handshakeObserver: () => handshakeCount++,
            v1InvokeObserver: () => v1InvokeCount++,
            v1FreeObserver: () => v1FreeCount++);

        using var transport = new WindowsMspNativeTransport(
            Options(AbsoluteLibraryPath()),
            new FakeLibraryLoader(library));

        Assert.Equal(MspNativeAbiMode.LengthDelimitedV2, transport.AbiMode);
        Assert.Equal(MspNativeAbiMode.LengthDelimitedV2, transport.NativeRuntimeInfo.AbiMode);
        Assert.Equal(2U, transport.NativeRuntimeInfo.MajorVersion);
        Assert.Equal(0U, transport.NativeRuntimeInfo.MinorVersion);
        Assert.Equal(MspNativeContract.AbiV2ContractId, transport.NativeRuntimeInfo.ContractId);
        Assert.Equal(abiInfo.Capabilities, transport.NativeRuntimeInfo.Capabilities);
        Assert.Equal("{}", Encoding.UTF8.GetString(
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray())));
        Assert.Equal(1, invokeCount);
        Assert.Equal(1, v2FreeCount);
        Assert.Equal(0, v1InvokeCount);
        Assert.Equal(0, v1FreeCount);
    }

    [Theory]
    [InlineData(MspNativeOperation.Execute, 1U)]
    [InlineData(MspNativeOperation.Parse, 2U)]
    [InlineData(MspNativeOperation.NormalizeWorkspacePath, 3U)]
    public void Invoke_v2_passes_operation_and_exact_length_without_nul_truncation(
        MspNativeOperation operation,
        uint expectedOperationCode)
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var request = new byte[] { (byte)'{', (byte)'"', (byte)'x', (byte)'"', (byte)':', 0, (byte)'}' };
        byte[]? capturedRequest = null;
        ulong capturedLength = 0;
        var freeCount = 0;
        var library = CreateV2Library(
            ValidAbiInfo(),
            (uint operationCode, nint requestPointer, ulong requestLength, ref nint response, ref ulong responseLength) =>
            {
                Assert.Equal(expectedOperationCode, operationCode);
                capturedLength = requestLength;
                capturedRequest = new byte[checked((int)requestLength)];
                Marshal.Copy(requestPointer, capturedRequest, 0, capturedRequest.Length);
                var bytes = new byte[] { (byte)'{', 0, (byte)'}' };
                response = Allocate(bytes);
                responseLength = (ulong)bytes.Length;
                return 0;
            },
            (pointer, length) =>
            {
                freeCount++;
                Assert.Equal(3UL, length);
                Marshal.FreeHGlobal(pointer);
            });

        using var transport = new WindowsMspNativeTransport(
            Options(AbsoluteLibraryPath()),
            new FakeLibraryLoader(library));

        var response = transport.Invoke(operation, request);

        Assert.Equal((ulong)request.Length, capturedLength);
        Assert.Equal(request, capturedRequest);
        Assert.Equal(new byte[] { (byte)'{', 0, (byte)'}' }, response);
        Assert.Equal(1, freeCount);
    }

    [Theory]
    [InlineData(0, 0UL, MspNativeFailureKind.NullResponse)]
    [InlineData(0, 7UL, MspNativeFailureKind.InvalidResponseBuffer)]
    [InlineData(1, 0UL, MspNativeFailureKind.InvalidResponseBuffer)]
    public void Invoke_v2_rejects_invalid_response_pointer_length_pairs_and_frees_once(
        int allocatePointer,
        ulong responseLength,
        MspNativeFailureKind expectedFailure)
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var freeCount = 0;
        nint freedPointer = nint.Zero;
        ulong freedLength = ulong.MaxValue;
        var library = CreateV2Library(
            ValidAbiInfo(),
            (uint _, nint _, ulong _, ref nint response, ref ulong length) =>
            {
                response = allocatePointer == 0 ? nint.Zero : Marshal.AllocHGlobal(1);
                length = responseLength;
                return 0;
            },
            (pointer, length) =>
            {
                freeCount++;
                freedPointer = pointer;
                freedLength = length;
                if (pointer != nint.Zero)
                {
                    Marshal.FreeHGlobal(pointer);
                }
            });
        using var transport = new WindowsMspNativeTransport(
            Options(AbsoluteLibraryPath()),
            new FakeLibraryLoader(library));

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));

        Assert.Equal(expectedFailure, exception.FailureKind);
        Assert.Equal(1, freeCount);
        Assert.Equal(allocatePointer == 0, freedPointer == nint.Zero);
        Assert.Equal(responseLength, freedLength);
    }

    [Theory]
    [InlineData(1, MspNativeFailureKind.NativeInvalidArgument)]
    [InlineData(2, MspNativeFailureKind.NativeUnsupportedOperation)]
    [InlineData(3, MspNativeFailureKind.NativeRequestTooLarge)]
    [InlineData(4, MspNativeFailureKind.ResponseTooLarge)]
    [InlineData(5, MspNativeFailureKind.NativeRuntimePanicked)]
    [InlineData(-1, MspNativeFailureKind.NativeStatusInvalid)]
    [InlineData(99, MspNativeFailureKind.NativeStatusInvalid)]
    public void Invoke_v2_maps_every_status_and_frees_null_buffer_once(
        int status,
        MspNativeFailureKind expectedFailure)
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var freeCount = 0;
        var library = CreateV2Library(
            ValidAbiInfo(),
            (uint _, nint _, ulong _, ref nint response, ref ulong responseLength) =>
            {
                response = nint.Zero;
                responseLength = 0;
                return status;
            },
            (pointer, length) =>
            {
                Assert.Equal(nint.Zero, pointer);
                Assert.Equal(0UL, length);
                freeCount++;
            });
        using var transport = new WindowsMspNativeTransport(
            Options(AbsoluteLibraryPath()),
            new FakeLibraryLoader(library));

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));

        Assert.Equal(expectedFailure, exception.FailureKind);
        Assert.Equal(1, freeCount);
    }

    [Theory]
    [InlineData(5UL)]
    [InlineData(2147483648UL)]
    public void Invoke_v2_rejects_response_length_over_limit_and_frees_exact_tuple_once(
        ulong responseLength)
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var freeCount = 0;
        var library = CreateV2Library(
            ValidAbiInfo(),
            (uint _, nint _, ulong _, ref nint response, ref ulong length) =>
            {
                response = Marshal.AllocHGlobal(1);
                length = responseLength;
                return 0;
            },
            (pointer, length) =>
            {
                freeCount++;
                Assert.Equal(responseLength, length);
                Marshal.FreeHGlobal(pointer);
            });
        using var transport = new WindowsMspNativeTransport(
            Options(
                AbsoluteLibraryPath(),
                new MspNativeAdapterLimits
                {
                    MaximumRequestBytes = 1024,
                    MaximumResponseBytes = 4,
                    MaximumDecodedStreamBytes = 1024
                }),
            new FakeLibraryLoader(library));

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));

        Assert.Equal(MspNativeFailureKind.ResponseTooLarge, exception.FailureKind);
        Assert.Equal(1, freeCount);
    }

    [Fact]
    public void Invoke_v2_frees_once_when_invoke_throws_after_returning_ownership()
    {
        var freeCount = 0;
        var library = new FakeNativeLibrary();
        var abiInfo = ValidAbiInfo();
        using var transport = new WindowsMspNativeTransport(
            new MspNativeAdapterLimits(),
            library,
            (uint _, nint _, ulong _, ref nint response, ref ulong responseLength) =>
            {
                var bytes = "{}"u8.ToArray();
                response = Allocate(bytes);
                responseLength = (ulong)bytes.Length;
                throw new InvalidOperationException(@"V:\private\invoke-secret");
            },
            (pointer, length) =>
            {
                freeCount++;
                Assert.Equal(2UL, length);
                Marshal.FreeHGlobal(pointer);
            },
            abiInfo);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));

        Assert.Equal(MspNativeFailureKind.InvocationFailed, exception.FailureKind);
        Assert.DoesNotContain("invoke-secret", exception.ToString());
        Assert.Equal(1, freeCount);
    }

    [Fact]
    public void Invoke_v2_sanitizes_free_failure_after_successful_copy()
    {
        const string secret = @"V:\private\v2-allocator-secret";
        var freeCount = 0;
        var library = new FakeNativeLibrary();
        using var transport = new WindowsMspNativeTransport(
            new MspNativeAdapterLimits(),
            library,
            (uint _, nint _, ulong _, ref nint response, ref ulong responseLength) =>
            {
                var bytes = "{}"u8.ToArray();
                response = Allocate(bytes);
                responseLength = (ulong)bytes.Length;
                return 0;
            },
            (pointer, _) =>
            {
                freeCount++;
                Marshal.FreeHGlobal(pointer);
                throw new InvalidOperationException(secret);
            },
            ValidAbiInfo());

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));

        Assert.Equal(MspNativeFailureKind.InvocationFailed, exception.FailureKind);
        Assert.DoesNotContain(secret, exception.ToString());
        Assert.Equal(1, freeCount);
    }

    [Fact]
    public void Invoke_v2_preserves_transport_failure_when_free_also_fails()
    {
        var freeCount = 0;
        var library = new FakeNativeLibrary();
        using var transport = new WindowsMspNativeTransport(
            new MspNativeAdapterLimits(),
            library,
            (uint _, nint _, ulong _, ref nint response, ref ulong responseLength) =>
            {
                response = nint.Zero;
                responseLength = 0;
                return (int)MspNativeInvokeStatusV2.Panic;
            },
            (_, _) =>
            {
                freeCount++;
                throw new InvalidOperationException("allocator detail");
            },
            ValidAbiInfo());

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));

        Assert.Equal(MspNativeFailureKind.NativeRuntimePanicked, exception.FailureKind);
        Assert.Equal(1, freeCount);
    }

    [Fact]
    public void Adapter_deserialization_failure_occurs_after_v2_buffer_is_freed_once()
    {
        var freeCount = 0;
        var library = new FakeNativeLibrary();
        var transport = new WindowsMspNativeTransport(
            new MspNativeAdapterLimits(),
            library,
            (uint _, nint _, ulong _, ref nint response, ref ulong responseLength) =>
            {
                var bytes = "{"u8.ToArray();
                response = Allocate(bytes);
                responseLength = (ulong)bytes.Length;
                return 0;
            },
            (pointer, _) =>
            {
                freeCount++;
                Marshal.FreeHGlobal(pointer);
            },
            ValidAbiInfo());
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.InvalidJson, exception.FailureKind);
        Assert.Equal(1, freeCount);
    }

    [Fact]
    public void Invoke_v2_rejects_invalid_or_oversized_request_before_native_call()
    {
        var invokeCount = 0;
        var freeCount = 0;
        var library = new FakeNativeLibrary();
        using var transport = new WindowsMspNativeTransport(
            new MspNativeAdapterLimits
            {
                MaximumRequestBytes = 2,
                MaximumResponseBytes = 1024,
                MaximumDecodedStreamBytes = 1024
            },
            library,
            (uint _, nint _, ulong _, ref nint response, ref ulong responseLength) =>
            {
                invokeCount++;
                response = nint.Zero;
                responseLength = 0;
                return 1;
            },
            (_, _) => freeCount++,
            ValidAbiInfo());

        Assert.Equal(
            MspNativeFailureKind.InvocationFailed,
            Assert.Throws<MspNativeAdapterException>(() =>
                transport.Invoke(MspNativeOperation.Execute, new byte[] { 0xff })).FailureKind);
        Assert.Equal(
            MspNativeFailureKind.InvocationFailed,
            Assert.Throws<MspNativeAdapterException>(() =>
                transport.Invoke(MspNativeOperation.Execute, "{}\n"u8.ToArray())).FailureKind);
        Assert.Equal(0, invokeCount);
        Assert.Equal(0, freeCount);
    }

    [Fact]
    public async Task Invoke_and_dispose_are_serialized_by_the_same_lock()
    {
        var enteredInvoke = new ManualResetEventSlim();
        var releaseInvoke = new ManualResetEventSlim();
        var library = new FakeNativeLibrary();
        var transport = new WindowsMspNativeTransport(
            new MspNativeAdapterLimits(),
            library,
            (uint _, nint _, ulong _, ref nint response, ref ulong responseLength) =>
            {
                enteredInvoke.Set();
                Assert.True(releaseInvoke.Wait(TimeSpan.FromSeconds(10)));
                var bytes = "{}"u8.ToArray();
                response = Allocate(bytes);
                responseLength = (ulong)bytes.Length;
                return 0;
            },
            (pointer, _) => Marshal.FreeHGlobal(pointer),
            ValidAbiInfo());

        var invokeTask = Task.Run(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));
        Assert.True(enteredInvoke.Wait(TimeSpan.FromSeconds(10)));
        var disposeTask = Task.Run(transport.Dispose);
        try
        {
            await Task.Delay(100);
            Assert.False(disposeTask.IsCompleted);
        }
        finally
        {
            releaseInvoke.Set();
        }

        Assert.Equal("{}", Encoding.UTF8.GetString(await invokeTask));
        await disposeTask;
        transport.Dispose();
        Assert.Equal(1, library.DisposeCount);
        Assert.Throws<ObjectDisposedException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));
    }

    [Fact]
    public void Invoke_passes_null_terminated_utf8_and_frees_returned_pointer_once()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        const string requestJson = "{\"message\":\"你好\"}";
        const string responseJson = "{\"ok\":true}";
        string? capturedRequest = null;
        var freeCount = 0;
        var library = CreateLibrary(
            requestPointer =>
            {
                capturedRequest = Marshal.PtrToStringUTF8(requestPointer);
                return AllocateNullTerminated(Encoding.UTF8.GetBytes(responseJson));
            },
            pointer =>
            {
                freeCount++;
                Marshal.FreeHGlobal(pointer);
            });

        using (var transport = new WindowsMspNativeTransport(
                   Options(AbsoluteLibraryPath()),
                   new FakeLibraryLoader(library)))
        {
            var response = transport.Invoke(
                MspNativeOperation.Execute,
                Encoding.UTF8.GetBytes(requestJson));

            Assert.Equal(requestJson, capturedRequest);
            Assert.Equal(responseJson, Encoding.UTF8.GetString(response));
            Assert.Equal(1, freeCount);
        }

        Assert.True(library.IsDisposed);
    }

    [Fact]
    public void Invoke_frees_returned_pointer_when_response_exceeds_limit()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var freeCount = 0;
        var library = CreateLibrary(
            _ => AllocateNullTerminated([1, 2, 3, 4, 5]),
            pointer =>
            {
                freeCount++;
                Marshal.FreeHGlobal(pointer);
            });
        using var transport = new WindowsMspNativeTransport(
            Options(
                AbsoluteLibraryPath(),
                new MspNativeAdapterLimits
                {
                    MaximumRequestBytes = 1024,
                    MaximumResponseBytes = 4,
                    MaximumDecodedStreamBytes = 1024
                }),
            new FakeLibraryLoader(library));

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));

        Assert.Equal(MspNativeFailureKind.ResponseTooLarge, exception.FailureKind);
        Assert.Equal(1, freeCount);
    }

    [Fact]
    public void Invoke_rejects_null_pointer_without_calling_free()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var freeCount = 0;
        var library = CreateLibrary(
            _ => nint.Zero,
            _ => freeCount++);
        using var transport = new WindowsMspNativeTransport(
            Options(AbsoluteLibraryPath()),
            new FakeLibraryLoader(library));

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));

        Assert.Equal(MspNativeFailureKind.NullResponse, exception.FailureKind);
        Assert.Equal(0, freeCount);
    }

    [Fact]
    public void Invoke_sanitizes_free_failure_after_successful_copy()
    {
        const string secret = @"V:\private\allocator-secret";
        var library = new FakeNativeLibrary();
        var jsonFunction = new MspNativeJsonFunction(_ =>
            AllocateNullTerminated("{}"u8.ToArray()));
        using var transport = new WindowsMspNativeTransport(
            new MspNativeAdapterLimits(),
            library,
            jsonFunction,
            jsonFunction,
            jsonFunction,
            pointer =>
            {
                Marshal.FreeHGlobal(pointer);
                throw new InvalidOperationException(secret);
            });

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));

        Assert.Equal(MspNativeFailureKind.InvocationFailed, exception.FailureKind);
        Assert.Null(exception.InnerException);
        Assert.DoesNotContain(secret, exception.ToString());
    }

    [Fact]
    public void Invoke_preserves_response_limit_failure_when_free_also_fails()
    {
        var library = new FakeNativeLibrary();
        var jsonFunction = new MspNativeJsonFunction(_ =>
            AllocateNullTerminated([1, 2, 3, 4, 5]));
        using var transport = new WindowsMspNativeTransport(
            new MspNativeAdapterLimits
            {
                MaximumRequestBytes = 1024,
                MaximumResponseBytes = 4,
                MaximumDecodedStreamBytes = 1024
            },
            library,
            jsonFunction,
            jsonFunction,
            jsonFunction,
            pointer =>
            {
                Marshal.FreeHGlobal(pointer);
                throw new InvalidOperationException("allocator detail");
            });

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}"u8.ToArray()));

        Assert.Equal(MspNativeFailureKind.ResponseTooLarge, exception.FailureKind);
    }

    [Fact]
    public void Invoke_rejects_oversized_request_before_native_call()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var invocationCount = 0;
        var library = CreateLibrary(
            _ =>
            {
                invocationCount++;
                return nint.Zero;
            },
            _ => { });
        using var transport = new WindowsMspNativeTransport(
            Options(
                AbsoluteLibraryPath(),
                new MspNativeAdapterLimits
                {
                    MaximumRequestBytes = 2,
                    MaximumResponseBytes = 1024,
                    MaximumDecodedStreamBytes = 1024
                }),
            new FakeLibraryLoader(library));

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, "{}\n"u8.ToArray()));

        Assert.Equal(MspNativeFailureKind.InvocationFailed, exception.FailureKind);
        Assert.Equal(0, invocationCount);
    }

    [Theory]
    [InlineData(new byte[] { 0xff })]
    [InlineData(new byte[] { (byte)'{', 0, (byte)'}' })]
    public void Invoke_rejects_non_utf8_or_embedded_null_request_before_native_call(byte[] request)
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var invocationCount = 0;
        var library = CreateLibrary(
            _ =>
            {
                invocationCount++;
                return nint.Zero;
            },
            _ => { });
        using var transport = new WindowsMspNativeTransport(
            Options(AbsoluteLibraryPath()),
            new FakeLibraryLoader(library));

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            transport.Invoke(MspNativeOperation.Execute, request));

        Assert.Equal(MspNativeFailureKind.InvocationFailed, exception.FailureKind);
        Assert.Equal(0, invocationCount);
    }

    [Fact]
    public void Library_options_reject_relative_and_non_dll_paths_before_loading()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var loader = new RecordingLibraryLoader();
        Assert.Equal(
            MspNativeFailureKind.LibraryUnavailable,
            Assert.Throws<MspNativeAdapterException>(() =>
                new WindowsMspNativeTransport(
                    Options("msp_core.dll"),
                    loader)).FailureKind);
        Assert.Equal(
            MspNativeFailureKind.LibraryUnavailable,
            Assert.Throws<MspNativeAdapterException>(() =>
                new WindowsMspNativeTransport(
                    Options(Path.Combine(Path.GetTempPath(), "msp_core.txt")),
                    loader)).FailureKind);
        Assert.Equal(0, loader.LoadCount);
    }

    private static FakeNativeLibrary CreateLibrary(
        MspNativeJsonFunction jsonFunction,
        MspNativeFreeStringFunction freeFunction)
    {
        var library = new FakeNativeLibrary();
        AddV1Exports(library, jsonFunction, freeFunction);
        return library;
    }

    private static FakeNativeLibrary CreateV2Library(
        MspAbiInfoV2 abiInfo,
        MspNativeInvokeV2Function invoke,
        MspNativeFreeBufferV2Function freeBuffer,
        int handshakeStatus = 0,
        bool includeV1 = false,
        Action? handshakeObserver = null,
        Action? v1InvokeObserver = null,
        Action? v1FreeObserver = null)
    {
        var library = new FakeNativeLibrary();
        library.Add(
            MspNativeContract.GetAbiInfoV2Export,
            new MspNativeGetAbiInfoV2Function(
                (ref MspAbiInfoV2 output, uint size) =>
                {
                    Assert.Equal(MspNativeContract.AbiV2InfoSize, size);
                    handshakeObserver?.Invoke();
                    output = abiInfo;
                    return handshakeStatus;
                }));
        library.Add(MspNativeContract.InvokeV2Export, invoke);
        library.Add(MspNativeContract.FreeBufferV2Export, freeBuffer);
        if (includeV1)
        {
            AddV1Exports(
                library,
                _ =>
                {
                    v1InvokeObserver?.Invoke();
                    return AllocateNullTerminated("{}"u8.ToArray());
                },
                pointer =>
                {
                    v1FreeObserver?.Invoke();
                    Marshal.FreeHGlobal(pointer);
                });
        }

        return library;
    }

    private static void AddSelectedV2Exports(FakeNativeLibrary library, int exportMask)
    {
        if ((exportMask & 1) != 0)
        {
            library.Add(
                MspNativeContract.GetAbiInfoV2Export,
                new MspNativeGetAbiInfoV2Function(
                    (ref MspAbiInfoV2 output, uint _) =>
                    {
                        output = ValidAbiInfo();
                        return 0;
                    }));
        }

        if ((exportMask & 2) != 0)
        {
            library.Add(
                MspNativeContract.InvokeV2Export,
                new MspNativeInvokeV2Function(DefaultV2Invoke));
        }

        if ((exportMask & 4) != 0)
        {
            library.Add(
                MspNativeContract.FreeBufferV2Export,
                new MspNativeFreeBufferV2Function(FreeV2));
        }
    }

    private static void AddV1Exports(
        FakeNativeLibrary library,
        MspNativeJsonFunction jsonFunction,
        MspNativeFreeStringFunction freeFunction)
    {
        library.Add(MspNativeContract.ExecuteExport, jsonFunction);
        library.Add(MspNativeContract.ParseExport, jsonFunction);
        library.Add(MspNativeContract.NormalizeWorkspacePathExport, jsonFunction);
        library.Add(MspNativeContract.FreeStringExport, freeFunction);
    }

    private static int DefaultV2Invoke(
        uint operation,
        nint request,
        ulong requestLength,
        ref nint response,
        ref ulong responseLength)
    {
        _ = operation;
        _ = request;
        _ = requestLength;
        var bytes = "{}"u8.ToArray();
        response = Allocate(bytes);
        responseLength = (ulong)bytes.Length;
        return 0;
    }

    private static void FreeV2(nint pointer, ulong _)
    {
        if (pointer != nint.Zero)
        {
            Marshal.FreeHGlobal(pointer);
        }
    }

    private static MspAbiInfoV2 ValidAbiInfo()
    {
        return new MspAbiInfoV2
        {
            Size = MspNativeContract.AbiV2InfoSize,
            MajorVersion = MspNativeContract.AbiV2MajorVersion,
            MinorVersion = MspNativeContract.AbiV2MinorVersion,
            Reserved = 0,
            ContractId = MspNativeContract.AbiV2ContractId,
            Capabilities = MspNativeContract.AbiV2RequiredCapabilities
        };
    }

    private static nint Allocate(byte[] value)
    {
        var pointer = Marshal.AllocHGlobal(value.Length);
        Marshal.Copy(value, 0, pointer, value.Length);
        return pointer;
    }

    private static nint AllocateNullTerminated(byte[] value)
    {
        var pointer = Marshal.AllocHGlobal(value.Length + 1);
        Marshal.Copy(value, 0, pointer, value.Length);
        Marshal.WriteByte(pointer, value.Length, 0);
        return pointer;
    }

    private static MspNativeLibraryOptions Options(
        string path,
        MspNativeAdapterLimits? limits = null)
    {
        return new MspNativeLibraryOptions
        {
            LibraryPath = path,
            Limits = limits ?? new MspNativeAdapterLimits()
        };
    }

    private static string AbsoluteLibraryPath()
    {
        return Path.Combine(Path.GetTempPath(), "msp_core.dll");
    }

    private sealed class FakeLibraryLoader(IMspNativeLibrary? library) : IMspNativeLibraryLoader
    {
        public IMspNativeLibrary? TryLoad(string absoluteLibraryPath)
        {
            return library;
        }
    }

    private sealed class RecordingLibraryLoader : IMspNativeLibraryLoader
    {
        public int LoadCount { get; private set; }

        public IMspNativeLibrary? TryLoad(string absoluteLibraryPath)
        {
            LoadCount++;
            return null;
        }
    }

    private sealed class ThrowingLibraryLoader(Exception exception) : IMspNativeLibraryLoader
    {
        public IMspNativeLibrary? TryLoad(string absoluteLibraryPath)
        {
            throw exception;
        }
    }

    private sealed class FakeNativeLibrary : IMspNativeLibrary
    {
        private readonly Dictionary<string, nint> exports = new(StringComparer.Ordinal);
        private readonly List<Delegate> keepAlive = [];

        public bool IsDisposed { get; private set; }

        public int DisposeCount { get; private set; }

        public Exception? DisposeException { get; set; }

        public List<string> QueriedExports { get; } = [];

        public void Add(string exportName, Delegate function)
        {
            keepAlive.Add(function);
            exports.Add(exportName, Marshal.GetFunctionPointerForDelegate(function));
        }

        public void AddAddress(string exportName, nint address)
        {
            exports.Add(exportName, address);
        }

        public bool TryGetExport(string exportName, out nint address)
        {
            QueriedExports.Add(exportName);
            return exports.TryGetValue(exportName, out address);
        }

        public void Dispose()
        {
            DisposeCount++;
            IsDisposed = true;
            if (DisposeException is not null)
            {
                throw DisposeException;
            }
        }
    }
}
