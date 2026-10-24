use git_explain::diff::LineRange;
use git_explain::language::{LanguageRegistry, SourceUnit, SourceUnitKind};
use std::path::Path;

fn extract_units(path: &str, source: &str, ranges: &[(usize, usize)]) -> Vec<SourceUnit> {
    LanguageRegistry::find_changed_units(
        Path::new(path),
        source,
        &ranges
            .iter()
            .map(|(start, end)| LineRange {
                start: *start,
                end: *end,
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

fn file_units(path: &str, source: &str) -> Vec<SourceUnit> {
    let line_count = source.lines().count().max(1);
    extract_units(path, source, &[(1, line_count)])
}

#[test]
fn path_rules_and_detection_for_all_families() {
    // 1. HCL
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("main.tf")),
        Some("HCL")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("variables.tfvars")),
        Some("HCL")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("infra.hcl")),
        Some("HCL")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(&Path::new("repo").join("infra").join("main.tf")),
        Some("HCL")
    );
    #[cfg(windows)]
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("C:\\repo\\infra\\main.tf")),
        Some("HCL")
    );

    // 2. YAML
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("deploy.yaml")),
        Some("YAML")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("values.yml")),
        Some("YAML")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(&Path::new("k8s").join("pod.yaml")),
        Some("YAML")
    );
    #[cfg(windows)]
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("D:\\k8s\\pod.yaml")),
        Some("YAML")
    );

    // 3. JSON & .tf.json
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("package.json")),
        Some("JSON")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("infra.tf.json")),
        Some("JSON")
    );

    // 4. JSONC
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("tsconfig.jsonc")),
        Some("JSONC")
    );

    // 5. Dockerfile & Containerfile variants
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("Dockerfile")),
        Some("Dockerfile")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("Dockerfile.prod")),
        Some("Dockerfile")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("Dockerfile.dev")),
        Some("Dockerfile")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("Containerfile")),
        Some("Dockerfile")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("Containerfile.ci")),
        Some("Dockerfile")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("app.Dockerfile")),
        Some("Dockerfile")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("service.Containerfile")),
        Some("Dockerfile")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(&Path::new("build").join("Dockerfile.stage")),
        Some("Dockerfile")
    );
    #[cfg(windows)]
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("C:\\build\\Dockerfile.stage")),
        Some("Dockerfile")
    );

    // 6. Bash
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("build.sh")),
        Some("Bash")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("test.bash")),
        Some("Bash")
    );

    // 7. PowerShell
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("setup.ps1")),
        Some("PowerShell")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("module.psm1")),
        Some("PowerShell")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("manifest.psd1")),
        Some("PowerShell")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(&Path::new("scripts").join("setup.psd1")),
        Some("PowerShell")
    );
    #[cfg(windows)]
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("D:\\scripts\\setup.psd1")),
        Some("PowerShell")
    );

    // 8. Kotlin
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("App.kt")),
        Some("Kotlin")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("build.gradle.kts")),
        Some("Kotlin")
    );
    assert_eq!(
        LanguageRegistry::language_for_path(&Path::new("app").join("Main.kt")),
        Some("Kotlin")
    );
    #[cfg(windows)]
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("E:\\app\\Main.kt")),
        Some("Kotlin")
    );

    // Negative tests: arbitrary extensionless files, shebang files, unrelated extensions
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("script")),
        None
    );
    assert_eq!(LanguageRegistry::language_for_path(Path::new("run")), None);
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("Makefile")),
        None
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("Dockerfile_backup")),
        None
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("Dockerfile.bak.txt")),
        None
    );
    assert_eq!(
        LanguageRegistry::language_for_path(Path::new("notes.txt")),
        None
    );
}

#[test]
fn hcl_extraction_blocks_attributes_and_heredoc() {
    let source = "terraform {\n  required_version = \">= 1.0\"\n}\n\nvariable \"region\" {\n  type = string\n  default = \"us-east-1\"\n}\n\nresource \"aws_s3_bucket\" \"logs\" {\n  bucket = \"my-logs\"\n  policy = <<-EOT\n  {\n    \"Version\": \"2012-10-17\"\n  }\n  EOT\n}\n";
    let units = file_units("main.tf", source);

    assert_eq!(units.len(), 3, "Expected 3 top-level HCL blocks");
    assert_eq!(units[0].name, "terraform");
    assert_eq!(units[0].start_line, 1);
    assert_eq!(units[0].end_line, 3);

    assert_eq!(units[1].name, "variable \"region\"");
    assert_eq!(units[1].start_line, 5);
    assert_eq!(units[1].end_line, 8);

    assert_eq!(units[2].name, "resource \"aws_s3_bucket\" \"logs\"");
    assert_eq!(units[2].start_line, 10);
    assert_eq!(units[2].end_line, 17);
    assert!(units[2].source.contains("<<-EOT"));
    assert!(units[2].source.contains("EOT"));

    // Diff mode: editing only variable
    let changed = extract_units("main.tf", source, &[(6, 6)]);
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].name, "variable \"region\"");
}

#[test]
fn yaml_extraction_mapping_entries_block_scalars_and_multidoc() {
    let source = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: app-cfg\ndata:\n  app.conf: |\n    server_name=localhost\n    port=8080\n---\napiVersion: apps/v1\nkind: Deployment\n";
    let units = file_units("deploy.yaml", source);

    // Multi-document YAML should extract top-level mapping entries per document without flattening nested structures
    assert!(
        units.len() >= 4,
        "Expected mapping entries from both documents, got {}",
        units.len()
    );
    let names: Vec<&str> = units.iter().map(|u| u.name.as_str()).collect();
    assert!(names.contains(&"apiVersion"));
    assert!(names.contains(&"kind"));
    assert!(names.contains(&"metadata"));
    assert!(names.contains(&"data"));

    // The data entry must keep the entire multiline block scalar intact
    let data_unit = units.iter().find(|u| u.name == "data").expect("data unit");
    assert_eq!(data_unit.start_line, 5);
    assert_eq!(data_unit.end_line, 8);
    assert!(data_unit.source.contains("app.conf: |"));
    assert!(data_unit.source.contains("port=8080"));

    // Diff selection targeting only the block scalar line
    let changed = extract_units("deploy.yaml", source, &[(7, 7)]);
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].name, "data");
}

#[test]
fn yaml_compact_single_line_prevents_duplicate_overlapping_sections() {
    let source = "foo: 1\nbar: 2\nbaz: 3\n";
    let units = file_units("compact.yaml", source);
    assert_eq!(units.len(), 3);
    assert_eq!(units[0].name, "foo");
    assert_eq!(units[0].start_line, 1);
    assert_eq!(units[0].end_line, 1);
    assert_eq!(units[1].name, "bar");
    assert_eq!(units[1].start_line, 2);
    assert_eq!(units[1].end_line, 2);
    assert_eq!(units[2].name, "baz");
    assert_eq!(units[2].start_line, 3);
    assert_eq!(units[2].end_line, 3);
}

#[test]
fn json_strict_syntax_and_root_array() {
    let source = "{\n  \"name\": \"git-explain\",\n  \"private\": true,\n  \"dependencies\": {\n    \"serde\": \"1.0\",\n    \"axum\": \"0.8\"\n  }\n}\n";
    let units = file_units("package.json", source);
    assert_eq!(units.len(), 3);
    assert_eq!(units[0].name, "\"name\"");
    assert_eq!(units[0].start_line, 2);
    assert_eq!(units[0].end_line, 2);

    assert_eq!(units[1].name, "\"private\"");
    assert_eq!(units[1].start_line, 3);
    assert_eq!(units[1].end_line, 3);

    assert_eq!(units[2].name, "\"dependencies\"");
    assert_eq!(units[2].start_line, 4);
    assert_eq!(units[2].end_line, 7);
    assert!(units[2].source.contains("\"axum\": \"0.8\""));

    // Strict JSON with trailing comma must NOT succeed as fine units
    let invalid_json = "{\n  \"key\": \"value\",\n}\n";
    let fallback = file_units("bad.json", invalid_json);
    // Malformed fallback stays a coarse original-source block, does not invent braces or panic
    assert_eq!(fallback.len(), 1);
    assert_eq!(fallback[0].start_line, 1);
    assert_eq!(fallback[0].end_line, 3);

    // Root array in JSON
    let array_json = "[\n  {\"id\": 1},\n  {\"id\": 2}\n]\n";
    let arr_units = file_units("array.json", array_json);
    assert_eq!(arr_units.len(), 1);
    assert_eq!(arr_units[0].start_line, 1);
    assert_eq!(arr_units[0].end_line, 4);
}

#[test]
fn json_compact_single_line_prevents_duplicate_overlapping_sections() {
    let source = "{\"a\": 1, \"b\": 2, \"c\": 3}";
    let units = file_units("compact.json", source);
    assert!(!units.is_empty());
    // All units must have unique ranges or names, no duplicate whole-line copies
    let mut names = std::collections::HashSet::new();
    for u in &units {
        assert!(names.insert(&u.name), "Duplicate unit name: {}", u.name);
    }
}

#[test]
fn jsonc_trailing_commas_comments_and_unicode_byte_offsets() {
    let source = "{\n  // 🚀 unicode comment\n  \"project\": \"🚀 launcher\",\n  /* multi-line\n     comment */\n  \"count\": 42, // trailing comma below\n  \"features\": [\n    \"a\",\n    \"b\", /* inner trailing comma */\n  ],\n}\n";
    let units = file_units("config.jsonc", source);

    assert_eq!(units.len(), 3);
    assert_eq!(units[0].name, "\"project\"");
    assert_eq!(units[0].start_line, 3);
    assert_eq!(units[0].end_line, 3);

    assert_eq!(units[1].name, "\"count\"");
    assert_eq!(units[1].start_line, 6);
    assert_eq!(units[1].end_line, 6);

    assert_eq!(units[2].name, "\"features\"");
    assert_eq!(units[2].start_line, 7);
    assert_eq!(units[2].end_line, 10);

    // Byte offsets must match original source slices exactly
    assert!(units[0].source.contains("\"project\": \"🚀 launcher\""));

    // JSON5 unquoted key must be rejected from fine analysis into coarse fallback
    let json5_source = "{\n  unquotedKey: \"value\",\n}\n";
    let json5_units = file_units("unquoted.jsonc", json5_source);
    assert_eq!(json5_units.len(), 1);
    assert_eq!(json5_units[0].kind, SourceUnitKind::TopLevelBlock);
}

#[test]
fn dockerfile_instructions_continuations_and_heredocs() {
    let source = "FROM alpine:3.18 AS builder\n\nENV APP_HOME=/app \\\n    PORT=8080\n\nRUN <<EOF\necho \"first line\"\necho \"second line\"\nEOF\n\nCOPY . /app\n\nENTRYPOINT [\"/app/run\"]\n";
    let units = file_units("Dockerfile", source);

    assert_eq!(units.len(), 5);
    assert_eq!(units[0].name, "FROM alpine:3.18 AS builder");
    assert_eq!(units[0].start_line, 1);
    assert_eq!(units[0].end_line, 1);

    // Multiline continuation instruction
    assert_eq!(units[1].name, "ENV APP_HOME=/app \\");
    assert_eq!(units[1].start_line, 3);
    assert_eq!(units[1].end_line, 4);
    assert!(units[1].source.contains("PORT=8080"));

    // Heredoc instruction
    assert_eq!(units[2].name, "RUN <<EOF");
    assert_eq!(units[2].start_line, 6);
    assert_eq!(units[2].end_line, 9);
    assert!(units[2].source.contains("first line"));
    assert!(units[2].source.contains("EOF"));

    assert_eq!(units[3].name, "COPY . /app");
    assert_eq!(units[3].start_line, 11);
    assert_eq!(units[3].end_line, 11);

    assert_eq!(units[4].name, "ENTRYPOINT [\"/app/run\"]");
    assert_eq!(units[4].start_line, 13);
    assert_eq!(units[4].end_line, 13);

    // Diff mode targeting inside the heredoc
    let changed = extract_units("Dockerfile.prod", source, &[(7, 8)]);
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].name, "RUN <<EOF");
}

#[test]
fn bash_functions_top_level_statements_heredocs_and_pipelines() {
    let source = "export ENV=production\n\ncat << 'EOF' > /tmp/msg.txt\nline 1\nline 2\nEOF\n\ncleanup() {\n  rm -f /tmp/msg.txt\n  echo \"cleaned\"\n}\n\ncat /var/log/app.log | grep ERROR | wc -l\n";
    let units = file_units("deploy.sh", source);

    assert_eq!(units.len(), 4);
    assert_eq!(units[0].name, "export ENV=production");
    assert_eq!(units[0].start_line, 1);
    assert_eq!(units[0].end_line, 1);

    // Heredoc statement
    assert_eq!(units[1].name, "cat << 'EOF' > /tmp/msg.txt");
    assert_eq!(units[1].start_line, 3);
    assert_eq!(units[1].end_line, 6);
    assert!(units[1].source.contains("line 1"));

    // Function definition
    assert_eq!(units[2].name, "cleanup");
    assert_eq!(units[2].kind, SourceUnitKind::Function);
    assert_eq!(units[2].start_line, 8);
    assert_eq!(units[2].end_line, 11);

    // Pipeline
    assert_eq!(units[3].name, "cat /var/log/app.log | grep ERROR | wc -l");
    assert_eq!(units[3].start_line, 13);
    assert_eq!(units[3].end_line, 13);
}

#[test]
fn powershell_functions_statements_here_strings_and_psd1() {
    let source = "$Config = @\"\nServer = localhost\nPort = 8080\n\"@\n\nfunction Invoke-Deploy {\n  param([string]$Env)\n  Write-Host \"Deploying to $Env\"\n}\n\nGet-Process | Where-Object { $_.CPU -gt 10 } | Select-Object -First 5\n";
    let units = file_units("deploy.ps1", source);

    assert_eq!(units.len(), 3);
    // Here-string variable assignment
    assert_eq!(units[0].start_line, 1);
    assert_eq!(units[0].end_line, 4);
    assert!(units[0].source.contains("@\""));
    assert!(units[0].source.contains("\"@"));

    // Function
    assert_eq!(units[1].name, "Invoke-Deploy");
    assert_eq!(units[1].kind, SourceUnitKind::Function);
    assert_eq!(units[1].start_line, 6);
    assert_eq!(units[1].end_line, 9);

    // Pipeline
    assert_eq!(units[2].start_line, 11);
    assert_eq!(units[2].end_line, 11);

    // .psd1 manifest syntax
    let psd1_source = "@{\n  ModuleVersion = '1.0.0'\n  Author = 'Admin'\n}\n";
    let psd1_units = file_units("module.psd1", psd1_source);
    assert_eq!(psd1_units.len(), 1);
    assert_eq!(psd1_units[0].start_line, 1);
    assert_eq!(psd1_units[0].end_line, 4);
}

#[test]
fn kotlin_declarations_suspend_extension_expression_body_and_kts() {
    let source = "package com.example\n\nval DEFAULT_TIMEOUT = 5000\n\nsuspend fun fetchData(url: String): String {\n    return client.get(url)\n}\n\nfun String.toSlug(): String = this.lowercase().replace(' ', '-')\n\nclass UserService(private val repo: Repo) {\n    fun getUser(id: String) = repo.find(id)\n}\n";
    let units = file_units("Service.kt", source);

    assert_eq!(units.len(), 4);
    assert_eq!(units[0].name, "DEFAULT_TIMEOUT");
    assert_eq!(units[0].kind, SourceUnitKind::Property);
    assert_eq!(units[0].start_line, 3);
    assert_eq!(units[0].end_line, 3);

    // Suspend function
    assert_eq!(units[1].name, "fetchData");
    assert_eq!(units[1].kind, SourceUnitKind::Function);
    assert_eq!(units[1].start_line, 5);
    assert_eq!(units[1].end_line, 7);

    // Extension + expression-body function
    assert_eq!(units[2].name, "toSlug");
    assert_eq!(units[2].kind, SourceUnitKind::Function);
    assert_eq!(units[2].start_line, 9);
    assert_eq!(units[2].end_line, 9);

    // Class declaration
    assert_eq!(units[3].name, "UserService");
    assert_eq!(units[3].kind, SourceUnitKind::Class);
    assert_eq!(units[3].start_line, 11);
    assert_eq!(units[3].end_line, 13);

    // .kts Kotlin script statements
    let kts_source = "plugins {\n    kotlin(\"jvm\") version \"1.9.0\"\n}\n\ndependencies {\n    implementation(\"org.jetbrains.kotlinx:kotlinx-coroutines-core:1.7.0\")\n}\n";
    let kts_units = file_units("build.gradle.kts", kts_source);
    assert_eq!(kts_units.len(), 2);
    assert_eq!(kts_units[0].start_line, 1);
    assert_eq!(kts_units[0].end_line, 3);
    assert_eq!(kts_units[1].start_line, 5);
    assert_eq!(kts_units[1].end_line, 7);
}

#[test]
fn infra_fallback_does_not_invent_brace_sections_on_malformed_input() {
    // Malformed shell/yaml with unbalanced braces should not synthesize fake struct/class
    let malformed_yaml = "services:\n  { malformed brace line\n  another line\n";
    let units = file_units("malformed.yaml", malformed_yaml);
    for u in &units {
        assert_ne!(u.kind, SourceUnitKind::Struct);
        assert_ne!(u.kind, SourceUnitKind::Class);
        assert_ne!(u.kind, SourceUnitKind::Interface);
    }
}

#[test]
fn shared_grammar_for_language_supports_all_aliases() {
    use git_explain::language::grammar::grammar_for_language;
    let aliases = [
        "rust",
        "rs",
        "python",
        "py",
        "go",
        "java",
        "c#",
        "cs",
        "csharp",
        "typescript",
        "ts",
        "tsx",
        "javascript",
        "js",
        "jsx",
        "c++",
        "cpp",
        "cxx",
        "cc",
        "c",
        "hcl",
        "terraform",
        "tf",
        "yaml",
        "yml",
        "json",
        "tf.json",
        "jsonc",
        "dockerfile",
        "containerfile",
        "docker",
        "bash",
        "sh",
        "shell",
        "powershell",
        "ps1",
        "psm1",
        "psd1",
        "kotlin",
        "kt",
        "kts",
    ];
    for alias in aliases {
        assert!(
            grammar_for_language(alias).is_some(),
            "Expected grammar for alias: {alias}"
        );
    }
    assert!(grammar_for_language("unknown_lang").is_none());
}
