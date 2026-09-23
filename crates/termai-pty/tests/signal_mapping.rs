mod common;

use termai_pty::{Sig, SignalOutcome};

// SignalOutcome contract (kernel/02 section 3.5) exercised on the pipe fallback:
// Int/Term fall back to the 0x03 byte and every other semantic signal is
// Unsupported. The native ConPTY mapping is wired in src/windows/conpty.rs and covered
// by the native tests. Never claim delivery when the platform cannot deliver.

#[test]
fn signal_outcomes_never_claim_unsupported_delivery() {
    let backend = common::pipes();
    let handle = backend
        .spawn(&common::shell_command(), common::size(), common::opts())
        .expect("spawn shell");
    for sig in [
        Sig::Hup,
        Sig::Quit,
        Sig::Usr1,
        Sig::Usr2,
        Sig::Winch,
        Sig::Stop,
        Sig::Cont,
    ] {
        assert_eq!(
            backend.signal(&handle, sig).expect("signal"),
            SignalOutcome::Unsupported,
            "{sig:?} must be Unsupported on this backend"
        );
    }
    match backend.signal(&handle, Sig::Int).expect("signal Int") {
        SignalOutcome::Delivered | SignalOutcome::ByteFallback(0x03) => {}
        other => panic!("signal(Int) must be Delivered or ByteFallback(0x03), got {other:?}"),
    }
    let tree = backend.process_tree(&handle).expect("process tree");
    let _ = backend.kill(&tree, termai_pty::KillMode::Force);
    backend.close(handle).expect("close");
}
