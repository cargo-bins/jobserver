use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use jobslot::Client;

#[test]
fn server_smoke() {
    let c = Client::new(1).unwrap();
    drop(c.acquire().unwrap());
    drop(c.acquire().unwrap());
}

#[test]
fn server_multiple() {
    let c = Client::new(2).unwrap();
    let a = c.acquire().unwrap();
    let b = c.acquire().unwrap();
    drop((a, b));
}

#[test]
fn server_available() {
    let c = t!(Client::new(10));
    assert_eq!(c.available().unwrap(), 10);
    let a = c.acquire().unwrap();
    assert_eq!(c.available().unwrap(), 9);
    drop(a);
    assert_eq!(c.available().unwrap(), 10);
}

#[test]
fn server_none_available() {
    let c = t!(Client::new(2));
    assert_eq!(c.available().unwrap(), 2);
    let a = c.acquire().unwrap();
    assert_eq!(c.available().unwrap(), 1);
    let b = c.acquire().unwrap();
    assert_eq!(c.available().unwrap(), 0);
    drop(a);
    assert_eq!(c.available().unwrap(), 1);
    drop(b);
    assert_eq!(c.available().unwrap(), 2);
}

#[test]
fn server_blocks() {
    let c = Client::new(1).unwrap();
    let a = c.acquire().unwrap();
    let hit = Arc::new(AtomicBool::new(false));
    let hit2 = hit.clone();
    let (tx, rx) = mpsc::channel();
    let t = thread::spawn(move || {
        tx.send(()).unwrap();
        let _b = c.acquire().unwrap();
        hit2.store(true, Ordering::SeqCst);
    });
    rx.recv().unwrap();
    assert!(!hit.load(Ordering::SeqCst));
    drop(a);
    t.join().unwrap();
    assert!(hit.load(Ordering::SeqCst));
}

#[test]
fn make_as_a_single_thread_client() {
    let c = Client::new(1).unwrap();
    let td = tempfile::tempdir().unwrap();

    let prog = env::var("MAKE").unwrap_or_else(|_| "make".to_string());
    let mut cmd = Command::new(prog);
    cmd.current_dir(td.path());

    File::create(td.path().join("Makefile"))
        .unwrap()
        .write_all(
            b"
all: foo bar
foo:
\techo foo
bar:
\techo bar
",
        )
        .unwrap();

    // The jobserver protocol means that the `make` process itself "runs with a
    // token", so we acquire our one token to drain the jobserver, and this
    // should mean that `make` itself never has a second token available to it.
    let _a = c.acquire();
    let output = c.configure_and_run(&mut cmd, |cmd| cmd.output()).unwrap();
    println!(
        "\n\t=== stderr\n\t\t{}",
        String::from_utf8_lossy(&output.stderr).replace('\n', "\n\t\t")
    );
    println!(
        "\t=== stdout\n\t\t{}",
        String::from_utf8_lossy(&output.stdout).replace('\n', "\n\t\t")
    );

    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let a = "\
echo foo
foo
echo bar
bar
";
    let b = "\
echo bar
bar
echo foo
foo
";

    assert!(stdout == a || stdout == b);
}

#[test]
fn make_as_a_multi_thread_client() {
    let c = Client::new(1).unwrap();
    let td = tempfile::tempdir().unwrap();

    let prog = env::var("MAKE").unwrap_or_else(|_| "make".to_string());
    let mut cmd = Command::new(prog);
    cmd.current_dir(td.path());

    File::create(td.path().join("Makefile"))
        .unwrap()
        .write_all(
            b"
all: foo bar
foo:
\techo foo
bar:
\techo bar
",
        )
        .unwrap();

    // We're leaking one extra token to `make` sort of violating the makefile
    // jobserver protocol. It has the desired effect though.
    let output = c.configure_and_run(&mut cmd, |cmd| cmd.output()).unwrap();
    println!(
        "\n\t=== stderr\n\t\t{}",
        String::from_utf8_lossy(&output.stderr).replace('\n', "\n\t\t")
    );
    println!(
        "\t=== stdout\n\t\t{}",
        String::from_utf8_lossy(&output.stdout).replace('\n', "\n\t\t")
    );

    assert!(output.status.success());
}

#[test]
fn zero_client() {
    let client = Client::new(0).unwrap();
    let (tx, rx) = mpsc::channel();
    let helper = client
        .into_helper_thread(move |a| drop(tx.send(a)))
        .unwrap();
    helper.request_token();
    helper.request_token();

    for _ in 0..1000 {
        assert!(rx.try_recv().is_err());
    }
}
