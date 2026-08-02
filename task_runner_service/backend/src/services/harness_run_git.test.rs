// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[tokio::test]
async fn cloud_run_branch_bootstraps_an_empty_harness_repository() {
    let root = TestDirectory::new("empty-cloud-branch");
    let bare_repo = root.path().join("harness.git");
    let prepare = root.path().join("prepare");

    run_git_output(
        vec![
            "init".to_string(),
            "--bare".to_string(),
            bare_repo.to_string_lossy().to_string(),
        ],
        None,
        &[],
    )
    .await
    .expect("initialize empty Harness repository");
    run_git_output(
        vec![
            "clone".to_string(),
            "--no-checkout".to_string(),
            bare_repo.to_string_lossy().to_string(),
            prepare.to_string_lossy().to_string(),
        ],
        None,
        &[],
    )
    .await
    .expect("clone empty Harness repository");

    let base_commit =
        create_cloud_run_branch(prepare.as_path(), "main", "chatos/runs/first", &[])
            .await
            .expect("bootstrap empty repository and create run branch");
    let refs = run_git_output(
        vec![
            "--git-dir".to_string(),
            bare_repo.to_string_lossy().to_string(),
            "show-ref".to_string(),
        ],
        None,
        &[],
    )
    .await
    .expect("read bootstrapped refs");

    assert!(refs.contains(format!("{base_commit} refs/heads/main").as_str()));
    assert!(refs.contains(format!("{base_commit} refs/heads/chatos/runs/first").as_str()));
}
