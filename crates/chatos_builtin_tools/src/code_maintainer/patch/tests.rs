use super::{apply_patch, apply_patch_limited};
use std::fs;
use std::path::PathBuf;

fn make_temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "code_maintainer_patch_test_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create temp root");
    root
}

#[test]
fn apply_patch_supports_unified_before_after_headers() {
    let root = make_temp_root();
    let target = root.join("a.txt");
    fs::write(&target, "line1\nline2\n").expect("write source file");

    let patch = "\
*** Begin Patch
*** Update File: a.txt
--- before
+++ after
@@ -1,2 +1,3 @@
 line1
 line2
+line3
*** End Patch";

    let result = apply_patch(&root, patch, true).expect("apply patch");
    assert_eq!(result.updated, vec!["a.txt"]);
    assert_eq!(
        fs::read_to_string(&target).expect("read target"),
        "line1\nline2\nline3\n"
    );

    fs::remove_dir_all(&root).expect("cleanup temp root");
}

#[test]
fn apply_patch_supports_multiple_operations_in_one_patch() {
    let root = make_temp_root();
    let update_target = root.join("update.txt");
    let delete_target = root.join("delete.txt");
    fs::write(&update_target, "old\n").expect("write update target");
    fs::write(&delete_target, "remove\n").expect("write delete target");

    let patch = "\
*** Begin Patch
*** Update File: update.txt
@@ -1 +1 @@
-old
+new
*** Add File: add.txt
+hello
*** Delete File: delete.txt
*** End Patch";

    let result = apply_patch(&root, patch, true).expect("apply patch");
    assert_eq!(result.updated, vec!["update.txt"]);
    assert_eq!(result.added, vec!["add.txt"]);
    assert_eq!(result.deleted, vec!["delete.txt"]);
    assert_eq!(
        fs::read_to_string(&update_target).expect("read updated file"),
        "new\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("add.txt")).expect("read added file"),
        "hello"
    );
    assert!(!delete_target.exists());

    fs::remove_dir_all(&root).expect("cleanup temp root");
}

#[test]
fn apply_patch_tolerates_extra_space_after_diff_marker() {
    let root = make_temp_root();
    let target = root.join("file6.cpp");
    fs::write(
        &target,
        "#include <iostream>\n// Test file 6\nint main() {\n    std::cout << \"Test file 6\" << std::endl;\n    return 0;\n}\n",
    )
    .expect("write cpp source");

    let patch = "\
*** Begin Patch
*** Update File: file6.cpp
---
  #include <iostream>
- // Test file 6
+ // Test file 11
  int main() {
-     std::cout << \"Test file 6\" << std::endl;
+     std::cout << \"Test file 11\" << std::endl;
      return 0;
  }
*** End Patch";

    let result = apply_patch(&root, patch, true).expect("apply patch");
    assert_eq!(result.updated, vec!["file6.cpp"]);
    let after = fs::read_to_string(&target).expect("read patched file");
    assert!(after.contains("// Test file 11"));
    assert!(after.contains("std::cout << \"Test file 11\" << std::endl;"));
    assert!(!after.contains("// Test file 6"));

    fs::remove_dir_all(&root).expect("cleanup temp root");
}

#[test]
fn apply_patch_supports_loose_replace_format_without_begin_patch() {
    let root = make_temp_root();
    let target = root.join("round2_file_1.txt");
    fs::write(&target, "Test file 7\n").expect("write source file");

    let patch = "\
Update File --- round2_file_1.txt
Test file 7
+++ round2_file_1.txt
Test file 18
End Patch";

    let result = apply_patch(&root, patch, true).expect("apply loose replace patch");
    assert_eq!(result.updated, vec!["round2_file_1.txt"]);
    assert_eq!(
        fs::read_to_string(&target).expect("read replaced file"),
        "Test file 18\n"
    );

    fs::remove_dir_all(&root).expect("cleanup temp root");
}

#[test]
fn apply_patch_loose_replace_requires_unique_match() {
    let root = make_temp_root();
    let target = root.join("ambiguous.txt");
    fs::write(&target, "same\nsame\n").expect("write source file");

    let patch = "\
Update File --- ambiguous.txt
same
+++ ambiguous.txt
new
End Patch";

    let err = apply_patch(&root, patch, true).expect_err("replace should be ambiguous");
    assert!(err.contains("multiple locations"));

    fs::remove_dir_all(&root).expect("cleanup temp root");
}

#[test]
fn apply_patch_limited_rejects_oversized_output() {
    let root = make_temp_root();
    let patch = "\
*** Begin Patch
*** Add File: large.txt
+12345
*** End Patch";

    let err = apply_patch_limited(&root, patch, true, 4).expect_err("oversized add should fail");
    assert!(err.contains("Patch target exceeds write limit"));

    fs::remove_dir_all(&root).expect("cleanup temp root");
}
