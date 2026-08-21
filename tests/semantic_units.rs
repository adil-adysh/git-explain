use git_explain::diff::LineRange;
use git_explain::language::{LanguageRegistry, SourceUnitKind};
use std::path::Path;

fn units(
    path: &str,
    source: &str,
    ranges: &[(usize, usize)],
) -> Vec<git_explain::language::SourceUnit> {
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

#[test]
fn rust_mixed_changes_include_import_struct_enum_method_and_constant() {
    let source = "use std::io;\n\nstruct Config {\n    timeout: u64,\n}\n\nenum Mode {\n    Fast,\n    Safe,\n}\n\nimpl Config {\n    fn load() {\n        read();\n    }\n}\n\nconst DEFAULT_TIMEOUT: u64 = 30;\n";
    let found = units(
        "src/config.rs",
        source,
        &[(1, 1), (4, 4), (8, 8), (13, 13), (18, 18)],
    );
    assert_eq!(found.len(), 5);
    assert!(found
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::ImportBlock));
    assert!(found
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Struct && unit.name == "Config"));
    assert!(found.iter().any(|unit| unit.kind == SourceUnitKind::Enum));
    assert!(found.iter().any(|unit| unit.kind == SourceUnitKind::Method
        && unit.name == "load"
        && unit.qualified_name.as_deref() == Some("Config::load")));
    assert!(found
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Constant));
}

#[test]
fn non_function_declarations_are_represented_across_languages() {
    let python = units(
        "service.py",
        "import os\nclass Service(Base):\n    TIMEOUT = 30\n",
        &[(1, 1), (2, 2), (3, 3)],
    );
    assert!(python
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::ImportBlock));
    assert!(python.iter().any(|unit| unit.kind == SourceUnitKind::Class));
    assert!(python
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Constant));

    let go = units(
        "model.go",
        "package model\ntype Repository interface {\n    Load() error\n}\n",
        &[(2, 3)],
    );
    assert!(go.iter().any(|unit| unit.kind == SourceUnitKind::Interface));

    let typescript = units("ids.ts", "import { User } from './user';\ntype UserId = string;\nconst defaultId: UserId = 'none';\n", &[(1, 1), (2, 2), (3, 3)]);
    assert!(typescript
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::ImportBlock));
    assert!(typescript
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::TypeAlias));
    assert!(typescript
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Constant));
}

#[test]
fn changed_line_outside_known_construct_gets_top_level_fallback() {
    let found = units("src/main.rs", "let value = compute();\n", &[(1, 1)]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, SourceUnitKind::Constant);
}

#[test]
fn parser_smoke_covers_java_csharp_c_and_cpp_declarations() {
    let java = units(
        "Service.java",
        "import java.util.List;\nclass Service {\n    int timeout = 30;\n}\n",
        &[(1, 1), (2, 2), (3, 3)],
    );
    assert!(java
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::ImportBlock));
    assert!(java.iter().any(|unit| unit.kind == SourceUnitKind::Class));
    assert!(java
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Constant));

    let csharp = units(
        "Service.cs",
        "using System;\npublic class Service {\n    private const int Timeout = 30;\n}\n",
        &[(1, 1), (2, 2), (3, 3)],
    );
    assert!(csharp
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::ImportBlock));
    assert!(csharp.iter().any(|unit| unit.kind == SourceUnitKind::Class));
    assert!(csharp
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Constant));

    let c = units(
        "config.c",
        "#include <stdint.h>\nstruct Config {\n    int timeout;\n};\n",
        &[(1, 1), (2, 2), (3, 3)],
    );
    assert!(c
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::ImportBlock));
    assert!(c.iter().any(|unit| unit.kind == SourceUnitKind::Struct));

    let cpp = units(
        "service.cpp",
        "#include <string>\nclass Service {\n    int timeout = 30;\n};\n",
        &[(1, 1), (2, 2), (3, 3)],
    );
    assert!(cpp
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::ImportBlock));
    assert!(cpp.iter().any(|unit| unit.kind == SourceUnitKind::Class));
    assert!(cpp.iter().any(|unit| unit.kind == SourceUnitKind::Constant));
}

#[test]
fn remaining_unit_categories_and_mixed_language_discovery_are_deterministic() {
    let rust = units(
        "lib.rs",
        "use std::fmt;\ntrait Render { fn render(&self); }\nimpl Render for Config {\n    fn render(&self) {}\n}\ntype Id = u64;\nstatic LIMIT: usize = 4;\nstruct Config;\n",
        &[(1, 1), (2, 2), (3, 3), (6, 6), (7, 7)],
    );
    assert!(rust.iter().any(|unit| unit.kind == SourceUnitKind::Trait));
    assert!(rust.iter().any(|unit| unit.kind == SourceUnitKind::Impl));
    assert!(rust
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::TypeAlias));
    assert!(rust
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Constant));

    let python = units(
        "service.py",
        "@dataclass\nclass Service(BaseService):\n    TIMEOUT = 30\n",
        &[(1, 1), (2, 2), (3, 3)],
    );
    assert!(python.iter().any(|unit| unit.kind == SourceUnitKind::Class));
    assert!(python
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Constant));

    let go = units(
        "model.go",
        "import \"time\"\ntype UserID = string\nvar timeout = time.Second\n",
        &[(1, 1), (2, 2), (3, 3)],
    );
    assert!(go
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::ImportBlock));
    assert!(go.iter().any(|unit| unit.kind == SourceUnitKind::TypeAlias));
    assert!(go.iter().any(|unit| unit.kind == SourceUnitKind::Constant));

    let typescript = units(
        "repo.ts",
        "import { User } from './user';\ninterface Repository { load(): User; }\nclass Service {}\n",
        &[(1, 1), (2, 2), (3, 3)],
    );
    assert!(typescript
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::ImportBlock));
    assert!(typescript
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Interface));
    assert!(typescript
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Class));

    let cpp = units(
        "service.cpp",
        "namespace auth { class Service {}; }\nusing UserId = unsigned long;\n",
        &[(1, 1), (2, 2)],
    );
    assert!(cpp
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::Namespace));
    assert!(cpp
        .iter()
        .any(|unit| unit.kind == SourceUnitKind::TypeAlias));
}
