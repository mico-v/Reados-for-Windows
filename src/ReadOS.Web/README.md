# ReadOS Web UI

`MSPChatUI/` is the product-owned browser UI package. It is served by the
Rust `msp-web-host` development adapter and is the runtime source for browser
access to ReadOS. The ignored `MSP/` checkout is reference material only; no
Web host or package command should use it as an input.

The package remains self-contained so the same renderer assets can be hosted
by a browser, WebView2, Android WebView, or another thin platform shell.
