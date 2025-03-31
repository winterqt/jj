// Copyright 2022 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::path::PathBuf;

use indoc::indoc;

use crate::common::CommandOutput;
use crate::common::TestEnvironment;
use crate::common::TestWorkDir;

#[test]
fn test_describe() {
    let mut test_env = TestEnvironment::default();
    let edit_script = test_env.set_up_fake_editor();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    // Set a description using `-m` flag
    let output = work_dir.run_jj(["describe", "-m", "description from CLI"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm 95979928 (empty) description from CLI
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");

    // Set the same description using `-m` flag, but with explicit newline
    let output = work_dir.run_jj(["describe", "-m", "description from CLI\n"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Nothing changed.
    [EOF]
    ");

    // Check that the text file gets initialized with the current description and
    // make no changes
    std::fs::write(&edit_script, "dump editor0").unwrap();
    let output = work_dir.run_jj(["describe"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Nothing changed.
    [EOF]
    ");
    insta::assert_snapshot!(
        std::fs::read_to_string(test_env.env_root().join("editor0")).unwrap(), @r#"
    description from CLI

    JJ: Lines starting with "JJ:" (like this one) will be removed.
    "#);

    // Set a description in editor
    std::fs::write(&edit_script, "write\ndescription from editor").unwrap();
    let output = work_dir.run_jj(["describe"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm 94fcb906 (empty) description from editor
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");

    // Lines in editor starting with "JJ: " are ignored
    std::fs::write(
        &edit_script,
        "write\nJJ: ignored\ndescription among comment\nJJ: ignored",
    )
    .unwrap();
    let output = work_dir.run_jj(["describe"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm 7a348923 (empty) description among comment
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");

    // Multi-line description
    std::fs::write(&edit_script, "write\nline1\nline2\n\nline4\n\n").unwrap();
    let output = work_dir.run_jj(["describe"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm 749361b5 (empty) line1
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");
    let output = work_dir.run_jj(["log", "--no-graph", "-r@", "-Tdescription"]);
    insta::assert_snapshot!(output, @r"
    line1
    line2

    line4
    [EOF]
    ");

    // Multi-line description again with CRLF, which should make no changes
    std::fs::write(&edit_script, "write\nline1\r\nline2\r\n\r\nline4\r\n\r\n").unwrap();
    let output = work_dir.run_jj(["describe"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Nothing changed.
    [EOF]
    ");

    // Multi-line description starting with newlines
    std::fs::write(&edit_script, "write\n\n\nline1\nline2").unwrap();
    let output = work_dir.run_jj(["describe"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm dc44dbee (empty) line1
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");
    let output = work_dir.run_jj(["log", "--no-graph", "-r@", "-Tdescription"]);
    insta::assert_snapshot!(output, @r"
    line1
    line2
    [EOF]
    ");

    // Clear description
    let output = work_dir.run_jj(["describe", "-m", ""]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm 6296963b (empty) (no description set)
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");
    std::fs::write(&edit_script, "write\n").unwrap();
    let output = work_dir.run_jj(["describe"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Nothing changed.
    [EOF]
    ");

    // Fails if the editor fails
    std::fs::write(&edit_script, "fail").unwrap();
    let output = work_dir.run_jj(["describe"]);
    insta::with_settings!({
        filters => [
            (r"\bEditor '[^']*'", "Editor '<redacted>'"),
            (r"\b(editor-)[^.]*(\.jjdescription)\b", "$1<redacted>$2"),
            ("exit code", "exit status"), // Windows
        ],
    }, {
        insta::assert_snapshot!(output, @r"
        ------- stderr -------
        Error: Failed to edit description
        Caused by: Editor '<redacted>' exited with exit status: 1
        Hint: Edited description is left in $TEST_ENV/repo/.jj/repo/editor-<redacted>.jjdescription
        [EOF]
        [exit status: 1]
        ");
    });

    // ignore everything after the first ignore-rest line
    std::fs::write(
        &edit_script,
        indoc! {"
            write
            description from editor

            content of message from editor
            JJ: ignore-rest
            content after ignore line should not be included
            JJ: ignore-rest
            ignore everything until EOF or next description
        "},
    )
    .unwrap();
    let output = work_dir.run_jj(["describe"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm 10fa2dc7 (empty) description from editor
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");
    let output = work_dir.run_jj(["log", "--no-graph", "-r@", "-Tdescription"]);
    insta::assert_snapshot!(output, @r"
    description from editor

    content of message from editor
    [EOF]
    ");
}

#[test]
fn test_describe_editor_env() {
    let test_env = TestEnvironment::default();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    // Fails if the editor doesn't exist
    let output = work_dir.run_jj_with(|cmd| {
        cmd.arg("describe")
            .env("EDITOR", "this-editor-does-not-exist")
    });
    insta::assert_snapshot!(
        output.normalize_stderr_with(|s| s.split_inclusive('\n').take(3).collect()), @r"
    ------- stderr -------
    Error: Failed to edit description
    Caused by:
    1: Failed to run editor 'this-editor-does-not-exist'
    [EOF]
    [exit status: 1]
    ");

    // `$VISUAL` overrides `$EDITOR`
    let output = work_dir.run_jj_with(|cmd| {
        cmd.arg("describe")
            .env("VISUAL", "bad-editor-from-visual-env")
            .env("EDITOR", "bad-editor-from-editor-env")
    });
    insta::assert_snapshot!(
        output.normalize_stderr_with(|s| s.split_inclusive('\n').take(3).collect()), @r"
    ------- stderr -------
    Error: Failed to edit description
    Caused by:
    1: Failed to run editor 'bad-editor-from-visual-env'
    [EOF]
    [exit status: 1]
    ");

    // `ui.editor` config overrides `$VISUAL`
    test_env.add_config(r#"ui.editor = "bad-editor-from-config""#);
    let output = work_dir.run_jj_with(|cmd| {
        cmd.arg("describe")
            .env("VISUAL", "bad-editor-from-visual-env")
    });
    insta::assert_snapshot!(
        output.normalize_stderr_with(|s| s.split_inclusive('\n').take(3).collect()), @r"
    ------- stderr -------
    Error: Failed to edit description
    Caused by:
    1: Failed to run editor 'bad-editor-from-config'
    [EOF]
    [exit status: 1]
    ");

    // `$JJ_EDITOR` overrides `ui.editor` config
    let output = work_dir.run_jj_with(|cmd| {
        cmd.arg("describe")
            .env("JJ_EDITOR", "bad-jj-editor-from-jj-editor-env")
    });
    insta::assert_snapshot!(
        output.normalize_stderr_with(|s| s.split_inclusive('\n').take(3).collect()), @r"
    ------- stderr -------
    Error: Failed to edit description
    Caused by:
    1: Failed to run editor 'bad-jj-editor-from-jj-editor-env'
    [EOF]
    [exit status: 1]
    ");
}

#[test]
fn test_describe_multiple_commits() {
    let mut test_env = TestEnvironment::default();
    let edit_script = test_env.set_up_fake_editor();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    // Initial setup
    work_dir.run_jj(["new"]).success();
    work_dir.run_jj(["new"]).success();
    insta::assert_snapshot!(get_log_output(&work_dir), @r"
    @  c6349e79bbfd
    ○  65b6b74e0897
    ○  230dd059e1b0
    ◆  000000000000
    [EOF]
    ");

    // Set the description of multiple commits using `-m` flag
    let output = work_dir.run_jj(["describe", "-r@", "-r@--", "-m", "description from CLI"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Updated 2 commits
    Rebased 1 descendant commits
    Working copy  (@) now at: kkmpptxz 41659b84 (empty) description from CLI
    Parent commit (@-)      : rlvkpnrz 8d650510 (empty) (no description set)
    [EOF]
    ");
    insta::assert_snapshot!(get_log_output(&work_dir), @r"
    @  41659b846096 description from CLI
    ○  8d650510daad
    ○  a42f5755e688 description from CLI
    ◆  000000000000
    [EOF]
    ");

    // Check that the text file gets initialized with the current description of
    // each commit and doesn't update commits if no changes are made.
    // Commit descriptions are edited in topological order
    std::fs::write(&edit_script, "dump editor0").unwrap();
    let output = work_dir.run_jj(["describe", "-r@", "@-"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Nothing changed.
    [EOF]
    ");
    insta::assert_snapshot!(
        std::fs::read_to_string(test_env.env_root().join("editor0")).unwrap(), @r#"
    JJ: Enter or edit commit descriptions after the `JJ: describe` lines.
    JJ: Warning:
    JJ: - The text you enter will be lost on a syntax error.
    JJ: - The syntax of the separator lines may change in the future.
    JJ:
    JJ: describe 8d650510daad -------


    JJ: describe 41659b846096 -------
    description from CLI

    JJ: Lines starting with "JJ:" (like this one) will be removed.
    "#);

    // Set the description of multiple commits in the editor
    std::fs::write(
        &edit_script,
        indoc! {"
            write
            JJ: Enter or edit commit descriptions after the `JJ: describe` lines.

            JJ: More header tests. Library tests verify parsing in other situations.

            JJ: describe 8d650510daad -------
            description from editor of @-

            further commit message of @-

            JJ: describe 41659b846096 -------
            description from editor of @

            further commit message of @

            JJ: Lines starting with \"JJ: \" (like this one) will be removed.
        "},
    )
    .unwrap();
    let output = work_dir.run_jj(["describe", "@", "@-"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Updated 2 commits
    Working copy  (@) now at: kkmpptxz f203494a (empty) description from editor of @
    Parent commit (@-)      : rlvkpnrz 0d76a92c (empty) description from editor of @-
    [EOF]
    ");
    insta::assert_snapshot!(get_log_output(&work_dir), @r"
    @  f203494a4507 description from editor of @
    │
    │  further commit message of @
    ○  0d76a92ca7cc description from editor of @-
    │
    │  further commit message of @-
    ○  a42f5755e688 description from CLI
    ◆  000000000000
    [EOF]
    ");

    // Fails if the edited message has a commit with multiple descriptions
    std::fs::write(
        &edit_script,
        indoc! {"
            write
            JJ: describe 0d76a92ca7cc -------
            first description from editor of @-

            further commit message of @-

            JJ: describe 0d76a92ca7cc -------
            second description from editor of @-

            further commit message of @-

            JJ: describe f203494a4507 -------
            updated description from editor of @

            further commit message of @

            JJ: Lines starting with \"JJ: \" (like this one) will be removed.
        "},
    )
    .unwrap();
    let output = work_dir.run_jj(["describe", "@", "@-"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Error: The following commits were found in the edited message multiple times: 0d76a92ca7cc
    [EOF]
    [exit status: 1]
    ");

    // Fails if the edited message has unexpected commit IDs
    std::fs::write(
        &edit_script,
        indoc! {"
            write
            JJ: describe 000000000000 -------
            unexpected commit ID

            JJ: describe 0d76a92ca7cc -------
            description from editor of @-

            further commit message of @-

            JJ: describe f203494a4507 -------
            description from editor of @

            further commit message of @

            JJ: Lines starting with \"JJ: \" (like this one) will be removed.
        "},
    )
    .unwrap();
    let output = work_dir.run_jj(["describe", "@", "@-"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Error: The following commits were not being edited, but were found in the edited message: 000000000000
    [EOF]
    [exit status: 1]
    ");

    // Fails if the edited message has missing commit messages
    std::fs::write(
        &edit_script,
        indoc! {"
            write
            JJ: describe f203494a4507 -------
            description from editor of @

            further commit message of @

            JJ: Lines starting with \"JJ: \" (like this one) will be removed.
        "},
    )
    .unwrap();
    let output = work_dir.run_jj(["describe", "@", "@-"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Error: The description for the following commits were not found in the edited message: 0d76a92ca7cc
    [EOF]
    [exit status: 1]
    ");

    // Fails if the edited message has a line which does not have any preceding
    // `JJ: describe` headers
    std::fs::write(
        &edit_script,
        indoc! {"
            write
            description from editor of @-

            JJ: describe f203494a4507 -------
            description from editor of @

            JJ: Lines starting with \"JJ: \" (like this one) will be removed.
        "},
    )
    .unwrap();
    let output = work_dir.run_jj(["describe", "@", "@-"]);
    insta::assert_snapshot!(output, @r#"
    ------- stderr -------
    Error: Found the following line without a commit header: "description from editor of @-"
    [EOF]
    [exit status: 1]
    "#);

    // Fails if the editor fails
    std::fs::write(&edit_script, "fail").unwrap();
    let output = work_dir.run_jj(["describe", "@", "@-"]);
    insta::with_settings!({
        filters => [
            (r"\bEditor '[^']*'", "Editor '<redacted>'"),
            (r"\b(editor-)[^.]*(\.jjdescription)\b", "$1<redacted>$2"),
            ("exit code", "exit status"), // Windows
        ],
    }, {
        insta::assert_snapshot!(output, @r"
        ------- stderr -------
        Error: Failed to edit description
        Caused by: Editor '<redacted>' exited with exit status: 1
        Hint: Edited description is left in $TEST_ENV/repo/.jj/repo/editor-<redacted>.jjdescription
        [EOF]
        [exit status: 1]
        ");
    });

    // describe lines should take priority over ignore-rest
    std::fs::write(
        &edit_script,
        indoc! {"
            write
            JJ: describe 0d76a92ca7cc -------
            description from editor for @-

            JJ: ignore-rest
            content after ignore-rest should not be included

            JJ: describe a42f5755e688 -------
            description from editor for @--

            JJ: ignore-rest
            each commit should skip their own ignore-rest
        "},
    )
    .unwrap();
    let output = work_dir.run_jj(["describe", "@-", "@--"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Updated 2 commits
    Rebased 1 descendant commits
    Working copy  (@) now at: kkmpptxz 1d7701ee (empty) description from editor of @
    Parent commit (@-)      : rlvkpnrz 5389926e (empty) description from editor for @-
    [EOF]
    ");
    insta::assert_snapshot!(get_log_output(&work_dir), @r"
    @  1d7701eec9bc description from editor of @
    │
    │  further commit message of @
    ○  5389926ebed6 description from editor for @-
    ○  eaa8547ae37a description from editor for @--
    ◆  000000000000
    [EOF]
    ");
}

#[test]
fn test_multiple_message_args() {
    let test_env = TestEnvironment::default();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    // Set a description using `-m` flag
    let output = work_dir.run_jj([
        "describe",
        "-m",
        "First Paragraph from CLI",
        "-m",
        "Second Paragraph from CLI",
    ]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm 99a36a50 (empty) First Paragraph from CLI
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");

    let output = work_dir.run_jj(["log", "--no-graph", "-r@", "-Tdescription"]);
    insta::assert_snapshot!(output, @r"
    First Paragraph from CLI

    Second Paragraph from CLI
    [EOF]
    ");

    // Set the same description, with existing newlines
    let output = work_dir.run_jj([
        "describe",
        "-m",
        "First Paragraph from CLI\n",
        "-m",
        "Second Paragraph from CLI\n",
    ]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Nothing changed.
    [EOF]
    ");

    // Use an empty -m flag between paragraphs to insert an extra blank line
    let output = work_dir.run_jj([
        "describe",
        "-m",
        "First Paragraph from CLI\n",
        "--message",
        "",
        "-m",
        "Second Paragraph from CLI",
    ]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm 01ac40b3 (empty) First Paragraph from CLI
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");

    let output = work_dir.run_jj(["log", "--no-graph", "-r@", "-Tdescription"]);
    insta::assert_snapshot!(output, @r"
    First Paragraph from CLI


    Second Paragraph from CLI
    [EOF]
    ");
}

#[test]
fn test_describe_default_description() {
    let mut test_env = TestEnvironment::default();
    let edit_script = test_env.set_up_fake_editor();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    test_env.add_config(r#"ui.default-description = "\n\nTESTED=TODO""#);
    let work_dir = test_env.work_dir("repo");

    work_dir.write_file("file1", "foo\n");
    work_dir.write_file("file2", "bar\n");
    std::fs::write(edit_script, ["dump editor"].join("\0")).unwrap();
    let output = work_dir.run_jj(["describe"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm 573b6df5 TESTED=TODO
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");
    insta::assert_snapshot!(
        std::fs::read_to_string(test_env.env_root().join("editor")).unwrap(), @r#"
    TESTED=TODO

    JJ: This commit contains the following changes:
    JJ:     A file1
    JJ:     A file2
    JJ:
    JJ: Lines starting with "JJ:" (like this one) will be removed.
    "#);
}

#[test]
fn test_describe_author() {
    let mut test_env = TestEnvironment::default();
    let edit_script = test_env.set_up_fake_editor();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    std::fs::write(edit_script, ["dump editor"].join("\0")).unwrap();

    test_env.add_config(indoc! {r#"
        [template-aliases]
        'format_signature(signature)' = 'signature.name() ++ " " ++ signature.email() ++ " " ++ signature.timestamp()'

        [templates]
        draft_commit_description = '''
        concat(
          description,
          "\n",
          indent(
            "JJ: ",
            concat(
              "Author: " ++ format_detailed_signature(author) ++ "\n",
              "Committer: " ++ format_detailed_signature(committer)  ++ "\n",
              "\n",
              diff.stat(76),
            ),
          ),
        )
        '''
    "#});
    let get_signatures = || {
        let template = r#"format_signature(author) ++ "\n" ++ format_signature(committer)"#;
        work_dir.run_jj(["log", "-r..", "-T", template])
    };

    // Initial setup
    work_dir.run_jj(["new"]).success();
    work_dir.run_jj(["new"]).success();
    work_dir.run_jj(["new"]).success();
    insta::assert_snapshot!(get_signatures(), @r"
    @  Test User test.user@example.com 2001-02-03 04:05:10.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:10.000 +07:00
    ○  Test User test.user@example.com 2001-02-03 04:05:09.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:09.000 +07:00
    ○  Test User test.user@example.com 2001-02-03 04:05:08.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:08.000 +07:00
    ○  Test User test.user@example.com 2001-02-03 04:05:07.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:07.000 +07:00
    ~
    [EOF]
    ");

    // Change the author for the latest commit (the committer is always reset)
    work_dir
        .run_jj([
            "describe",
            "--author",
            "Super Seeder <super.seeder@example.com>",
        ])
        .success();
    insta::assert_snapshot!(get_signatures(), @r"
    @  Super Seeder super.seeder@example.com 2001-02-03 04:05:12.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:12.000 +07:00
    ○  Test User test.user@example.com 2001-02-03 04:05:09.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:09.000 +07:00
    ○  Test User test.user@example.com 2001-02-03 04:05:08.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:08.000 +07:00
    ○  Test User test.user@example.com 2001-02-03 04:05:07.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:07.000 +07:00
    ~
    [EOF]
    ");
    insta::assert_snapshot!(
        std::fs::read_to_string(test_env.env_root().join("editor")).unwrap(), @r#"
    JJ: Author: Super Seeder <super.seeder@example.com> (2001-02-03 08:05:12)
    JJ: Committer: Test User <test.user@example.com> (2001-02-03 08:05:12)

    JJ: 0 files changed, 0 insertions(+), 0 deletions(-)
    JJ:
    JJ: Lines starting with "JJ:" (like this one) will be removed.
    "#);

    // Change the author for multiple commits (the committer is always reset)
    work_dir
        .run_jj([
            "describe",
            "@---",
            "@-",
            "--no-edit",
            "--author",
            "Super Seeder <super.seeder@example.com>",
        ])
        .success();
    insta::assert_snapshot!(get_signatures(), @r"
    @  Super Seeder super.seeder@example.com 2001-02-03 04:05:12.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:14.000 +07:00
    ○  Super Seeder super.seeder@example.com 2001-02-03 04:05:14.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:14.000 +07:00
    ○  Test User test.user@example.com 2001-02-03 04:05:14.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:14.000 +07:00
    ○  Super Seeder super.seeder@example.com 2001-02-03 04:05:14.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:14.000 +07:00
    ~
    [EOF]
    ");

    // Reset the author for the latest commit (the committer is always reset)
    work_dir
        .run_jj([
            "describe",
            "--config=user.name=Ove Ridder",
            "--config=user.email=ove.ridder@example.com",
            "--no-edit",
            "--reset-author",
        ])
        .success();
    insta::assert_snapshot!(get_signatures(), @r"
    @  Ove Ridder ove.ridder@example.com 2001-02-03 04:05:16.000 +07:00
    │  Ove Ridder ove.ridder@example.com 2001-02-03 04:05:16.000 +07:00
    ○  Super Seeder super.seeder@example.com 2001-02-03 04:05:14.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:14.000 +07:00
    ○  Test User test.user@example.com 2001-02-03 04:05:14.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:14.000 +07:00
    ○  Super Seeder super.seeder@example.com 2001-02-03 04:05:14.000 +07:00
    │  Test User test.user@example.com 2001-02-03 04:05:14.000 +07:00
    ~
    [EOF]
    ");

    // Reset the author for multiple commits (the committer is always reset)
    work_dir
        .run_jj([
            "describe",
            "@---",
            "@-",
            "--config=user.name=Ove Ridder",
            "--config=user.email=ove.ridder@example.com",
            "--reset-author",
        ])
        .success();
    insta::assert_snapshot!(get_signatures(), @r"
    @  Ove Ridder ove.ridder@example.com 2001-02-03 04:05:18.000 +07:00
    │  Ove Ridder ove.ridder@example.com 2001-02-03 04:05:18.000 +07:00
    ○  Ove Ridder ove.ridder@example.com 2001-02-03 04:05:18.000 +07:00
    │  Ove Ridder ove.ridder@example.com 2001-02-03 04:05:18.000 +07:00
    ○  Test User test.user@example.com 2001-02-03 04:05:14.000 +07:00
    │  Ove Ridder ove.ridder@example.com 2001-02-03 04:05:18.000 +07:00
    ○  Ove Ridder ove.ridder@example.com 2001-02-03 04:05:18.000 +07:00
    │  Ove Ridder ove.ridder@example.com 2001-02-03 04:05:18.000 +07:00
    ~
    [EOF]
    ");
    insta::assert_snapshot!(
        std::fs::read_to_string(test_env.env_root().join("editor")).unwrap(), @r#"
    JJ: Enter or edit commit descriptions after the `JJ: describe` lines.
    JJ: Warning:
    JJ: - The text you enter will be lost on a syntax error.
    JJ: - The syntax of the separator lines may change in the future.
    JJ:
    JJ: describe eae86afaa20c -------

    JJ: Author: Ove Ridder <ove.ridder@example.com> (2001-02-03 08:05:18)
    JJ: Committer: Ove Ridder <ove.ridder@example.com> (2001-02-03 08:05:18)

    JJ: 0 files changed, 0 insertions(+), 0 deletions(-)
    JJ:
    JJ: describe ba485659f76a -------

    JJ: Author: Ove Ridder <ove.ridder@example.com> (2001-02-03 08:05:18)
    JJ: Committer: Ove Ridder <ove.ridder@example.com> (2001-02-03 08:05:18)

    JJ: 0 files changed, 0 insertions(+), 0 deletions(-)
    JJ:
    JJ: Lines starting with "JJ:" (like this one) will be removed.
    "#);
}

#[test]
fn test_describe_avoids_unc() {
    let mut test_env = TestEnvironment::default();
    let edit_script = test_env.set_up_fake_editor();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    std::fs::write(edit_script, "dump-path path").unwrap();
    work_dir.run_jj(["describe"]).success();

    let edited_path =
        PathBuf::from(std::fs::read_to_string(test_env.env_root().join("path")).unwrap());
    // While `assert!(!edited_path.starts_with("//?/"))` could work here in most
    // cases, it fails when it is not safe to strip the prefix, such as paths
    // over 260 chars.
    assert_eq!(edited_path, dunce::simplified(&edited_path));
}

#[test]
fn test_describe_with_edit_and_message_args_opens_editor() {
    let mut test_env = TestEnvironment::default();
    let edit_script = test_env.set_up_fake_editor();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    std::fs::write(edit_script, ["dump editor"].join("\0")).unwrap();
    let output = work_dir.run_jj(["describe", "-m", "message from command line", "--edit"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm 61ece7a9 (empty) message from command line
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");
    insta::assert_snapshot!(
        std::fs::read_to_string(test_env.env_root().join("editor")).unwrap(), @r#"
    message from command line

    JJ: Lines starting with "JJ:" (like this one) will be removed.
    "#);
}

#[test]
fn test_describe_change_with_existing_message_with_edit_and_message_args_opens_editor() {
    let mut test_env = TestEnvironment::default();
    let edit_script = test_env.set_up_fake_editor();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    work_dir
        .run_jj(["describe", "-m", "original message"])
        .success();

    std::fs::write(edit_script, ["dump editor"].join("\0")).unwrap();
    let output = work_dir.run_jj(["describe", "-m", "new message", "--edit"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    Working copy  (@) now at: qpvuntsm de694560 (empty) new message
    Parent commit (@-)      : zzzzzzzz 00000000 (empty) (no description set)
    [EOF]
    ");
    insta::assert_snapshot!(
        std::fs::read_to_string(test_env.env_root().join("editor")).unwrap(), @r#"
    new message

    JJ: Lines starting with "JJ:" (like this one) will be removed.
    "#);
}

#[test]
fn test_edit_cannot_be_used_with_no_edit() {
    let test_env = TestEnvironment::default();
    test_env.run_jj_in(".", ["git", "init", "repo"]).success();
    let work_dir = test_env.work_dir("repo");

    let output = work_dir.run_jj(["describe", "--no-edit", "--edit"]);
    insta::assert_snapshot!(output, @r"
    ------- stderr -------
    error: the argument '--no-edit' cannot be used with '--edit'

    Usage: jj describe --no-edit [REVSETS]...

    For more information, try '--help'.
    [EOF]
    [exit status: 2]
    ");
}

#[must_use]
fn get_log_output(work_dir: &TestWorkDir) -> CommandOutput {
    let template = r#"commit_id.short() ++ " " ++ description"#;
    work_dir.run_jj(["log", "-T", template])
}
