use super::*;

#[test]
fn placeholders_are_literal_single_pass_and_unknown_variables_fail() {
    let values = BTreeMap::from([(
        "workspace.path",
        "/tmp/a b; $(touch nope) {{unknown}}".into(),
    )]);
    assert_eq!(
        expand("--folder={{workspace.path}}", &values).unwrap(),
        "--folder=/tmp/a b; $(touch nope) {{unknown}}"
    );
    assert!(expand("{{missing}}", &values).is_err());
    assert!(expand("{{workspace.path", &values).is_err());
}

#[test]
fn external_command_launches_without_a_pty_and_reports_failure_and_reaps() {
    let root = std::env::temp_dir().join(format!("external-action-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let prepared = Prepared { workspace_id: 7, name: "External".into(), config: TabConfig {
        executable: Some("/usr/bin/python3".into()),
        args: vec!["-c".into(), "import os,sys; assert not os.isatty(0); assert os.getsid(0)==os.getpid(); assert sys.argv[1]=='literal ; $(touch nope)'; open('result','w').write(os.environ['ACTION_TEST']); sys.exit(7)".into(), "literal ; $(touch nope)".into()],
        env: BTreeMap::from([("ACTION_TEST".into(), "works".into())]), cwd_override: Some(root.to_str().unwrap().into()), ..Default::default()
    }};
    launch(
        prepared,
        Arc::new(move |exit| {
            send.send(exit).unwrap();
        }),
    )
    .unwrap();
    let exit = receive
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(exit.code, Some(7));
    assert_eq!(
        std::fs::read_to_string(root.join("result")).unwrap(),
        "works"
    );
    assert!(!root.join("nope").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_executable_and_directory_are_reported_without_leaking_launch_slots() {
    for cwd in ["/tmp", "/definitely-missing-external-action-directory"] {
        for _ in 0..65 {
            let prepared = Prepared {
                workspace_id: 1,
                name: "Invalid".into(),
                config: TabConfig {
                    executable: Some("/definitely-missing-external-action-executable".into()),
                    cwd_override: Some(cwd.into()),
                    ..Default::default()
                },
            };
            let error =
                launch(prepared, Arc::new(|_| panic!("No child should start"))).unwrap_err();
            assert!(!error.contains("Too many"));
        }
    }
}
