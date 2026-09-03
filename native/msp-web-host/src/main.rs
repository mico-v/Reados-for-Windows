use msp_web_host::MspWebHost;

fn main() {
    let mut bind = "127.0.0.1:8787".to_string();
    let mut root = "src/ReadOS.Web/MSPChatUI".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--bind" {
            bind = args.next().unwrap_or(bind);
        } else if argument == "--root" {
            root = args.next().unwrap_or(root);
        } else if argument == "--help" {
            println!(
                "Usage: reados-msp-web [--bind 127.0.0.1:8787] [--root src/ReadOS.Web/MSPChatUI]"
            );
            return;
        }
    }
    let host = MspWebHost::new()
        .expect("portable MSP host initialization")
        .with_static_root(root);
    host.serve(&bind).expect("MSP web host stopped");
}
