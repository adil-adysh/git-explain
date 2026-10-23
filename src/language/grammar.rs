//! Shared Tree-sitter grammar selection for all supported languages.

pub fn grammar_for_language(language: &str) -> Option<tree_sitter::Language> {
    match language.to_lowercase().as_str() {
        "rust" | "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "python" | "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        "c#" | "cs" | "csharp" => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        "typescript" | "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "javascript" | "js" | "jsx" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "c++" | "cpp" | "cxx" | "cc" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "c" => Some(tree_sitter_c::LANGUAGE.into()),
        "hcl" | "terraform" | "tf" => Some(tree_sitter_hcl::LANGUAGE.into()),
        "yaml" | "yml" => Some(tree_sitter_yaml::LANGUAGE.into()),
        "json" | "tf.json" => Some(tree_sitter_json::LANGUAGE.into()),
        "jsonc" => Some(tree_sitter_json::LANGUAGE.into()),
        "dockerfile" | "containerfile" | "docker" => {
            Some(tree_sitter_containerfile::LANGUAGE.into())
        }
        "bash" | "sh" | "shell" => Some(tree_sitter_bash::LANGUAGE.into()),
        "powershell" | "ps1" | "psm1" | "psd1" => Some(tree_sitter_powershell::LANGUAGE.into()),
        "kotlin" | "kt" | "kts" => Some(tree_sitter_kotlin_ng::LANGUAGE.into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_all_grammars() {
        let languages = [
            "rust",
            "python",
            "go",
            "java",
            "c#",
            "typescript",
            "tsx",
            "javascript",
            "c++",
            "c",
            "hcl",
            "yaml",
            "json",
            "jsonc",
            "dockerfile",
            "bash",
            "powershell",
            "kotlin",
        ];
        for lang in languages {
            let grammar = grammar_for_language(lang);
            assert!(grammar.is_some(), "Failed to get grammar for {lang}");
            let mut parser = tree_sitter::Parser::new();
            assert!(
                parser.set_language(&grammar.unwrap()).is_ok(),
                "Failed to set language for {lang}"
            );
        }
    }
}
