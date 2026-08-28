use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "git-explain",
    about = "Read and understand code changed in the working tree or a Git commit",
    after_help = "Examples:\n  git explain                 Explain current changes\n  git explain HEAD            Explain an existing commit\n  git explain --direct        Bypass the background daemon\n  git explain --debug         Inspect changes without opening a browser\n\nHelp:\n  git explain -h              Show command help\n  git explain config -h       Show configuration help\n  git explain profile -h      Show profile help"
)]
pub struct Cli {
    #[arg(
        value_name = "REVISION",
        help = "Explain a Git commit instead of the working tree"
    )]
    pub revision: Option<String>,
    #[arg(
        short = 'd',
        long,
        help = "Print changed supported-language functions and exit"
    )]
    pub debug: bool,
    #[arg(
        long,
        help = "Override the configured model profile for this invocation"
    )]
    pub profile: Option<String>,
    #[arg(long, help = "Override the configured web-server port")]
    pub port: Option<u16>,
    #[arg(long, help = "Bypass the background daemon and use a one-shot server")]
    pub direct: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    #[command(alias = "cfg", about = "Initialize and inspect configuration")]
    Config(ConfigCommand),
    #[command(
        alias = "prof",
        about = "Create, select, inspect, and test model profiles"
    )]
    Profile(ProfileCommand),
    #[command(about = "Inspect or clear the explanation cache")]
    Cache(CacheCommand),
    #[command(about = "Inspect local Ollama context history and recommendations")]
    Context(ContextCommand),
    #[command(about = "Start, inspect, refresh, or stop the local daemon")]
    Daemon(DaemonCommand),
}

#[derive(Clone, Debug, Args)]
pub struct ContextCommand {
    #[command(subcommand)]
    pub action: ContextAction,
}
#[derive(Clone, Debug, Subcommand)]
pub enum ContextAction {
    Stats,
    Recommend,
    Reset {
        #[arg(long)]
        force: bool,
    },
}

#[derive(Clone, Debug, Args)]
pub struct DaemonCommand {
    #[command(subcommand)]
    pub action: DaemonAction,
}

#[derive(Clone, Debug, Subcommand)]
pub enum DaemonAction {
    #[command(about = "Start the daemon if it is not already running")]
    Start,
    #[command(about = "Stop the daemon gracefully")]
    Stop,
    #[command(about = "Show daemon health and endpoint details")]
    Status,
    #[command(about = "Reanalyze the active repository without model inference")]
    Refresh,
    #[command(about = "Run the daemon in the foreground")]
    Run,
}

#[derive(Clone, Debug, Args)]
pub struct CacheCommand {
    #[command(subcommand)]
    pub action: CacheAction,
}
#[derive(Clone, Debug, Subcommand)]
pub enum CacheAction {
    Status,
    Clear,
}

#[derive(Clone, Debug, Args)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Clone, Debug, Subcommand)]
pub enum ConfigAction {
    #[command(about = "Show the resolved configuration with secrets redacted")]
    Show,
    #[command(about = "Show the user and repository config paths")]
    Path,
    #[command(
        about = "Create a documented user template, or a restricted repository-safe template with --repo"
    )]
    Init {
        #[arg(
            short = 'f',
            long,
            help = "Overwrite configuration if it already exists"
        )]
        force: bool,
        #[arg(
            short = 'r',
            long,
            help = "Create the restricted configuration template for the current repository"
        )]
        repo: bool,
    },
    #[command(about = "Edit configuration through an accessible numbered menu")]
    Edit {
        #[arg(
            short = 'r',
            long,
            help = "Edit repository-safe configuration for the current repository"
        )]
        repo: bool,
    },
    #[command(about = "Update reader preferences (persisted; --repo is repository scoped)")]
    Reader {
        #[arg(long)]
        experience: Option<String>,
        #[arg(long)]
        add_known_language: Vec<String>,
        #[arg(long)]
        remove_known_language: Vec<String>,
        #[arg(long)]
        clear_known_languages: bool,
        #[arg(long)]
        add_learning_language: Vec<String>,
        #[arg(long)]
        remove_learning_language: Vec<String>,
        #[arg(long)]
        clear_learning_languages: bool,
        #[arg(long)]
        add_known_framework: Vec<String>,
        #[arg(long)]
        remove_known_framework: Vec<String>,
        #[arg(long)]
        clear_known_frameworks: bool,
        #[arg(long)]
        add_learning_framework: Vec<String>,
        #[arg(long)]
        remove_learning_framework: Vec<String>,
        #[arg(long)]
        clear_learning_frameworks: bool,
        #[arg(short = 'r', long)]
        repo: bool,
    },
    #[command(about = "Update explanation preferences")]
    Explanation {
        #[arg(long)]
        depth: Option<String>,
        #[arg(long)]
        annotation_limit: Option<u32>,
        #[arg(long)]
        annotation_word_limit: Option<u32>,
        #[arg(long)]
        explain_language_concepts: Option<bool>,
        #[arg(long)]
        explain_framework_concepts: Option<bool>,
        #[arg(long)]
        infer_intent: Option<bool>,
        #[arg(short = 'r', long)]
        repo: bool,
    },
    #[command(about = "Update cache preferences")]
    Cache {
        #[arg(long)]
        enabled: Option<bool>,
        #[arg(short = 'r', long)]
        repo: bool,
    },
    #[command(about = "Update persisted local server settings; top-level --port is runtime-only")]
    Server {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        open_browser: Option<bool>,
        #[arg(short = 'r', long)]
        repo: bool,
    },
    #[command(about = "Update Git analysis preferences")]
    Git {
        #[arg(long)]
        diff_target: Option<String>,
        #[arg(long)]
        include_staged: Option<bool>,
        #[arg(long)]
        include_untracked: Option<bool>,
        #[arg(short = 'r', long)]
        repo: bool,
    },
    #[command(about = "Select an existing trusted model profile; profile use remains preferred")]
    Model {
        #[arg(long, conflicts_with = "clear_profile")]
        profile: Option<String>,
        #[arg(long, conflicts_with = "profile")]
        clear_profile: bool,
        #[arg(short = 'r', long)]
        repo: bool,
    },
}

#[derive(Clone, Debug, Args)]
pub struct ProfileCommand {
    #[command(subcommand)]
    pub action: ProfileAction,
}

#[derive(Clone, Debug, Subcommand)]
pub enum ProfileAction {
    #[command(about = "List profiles stored in trusted user configuration")]
    List,
    #[command(about = "Show a profile with credential values redacted")]
    Show { name: String },
    #[command(about = "Make a profile the user default or repository preference")]
    Use {
        name: String,
        #[arg(
            short = 'r',
            long,
            help = "Select this profile for the current repository"
        )]
        repo: bool,
    },
    #[command(about = "Check endpoint connectivity without sending repository source")]
    Test { name: String },
    #[command(
        about = "Add a trusted user profile",
        before_help = "With no name or options in an interactive terminal, starts an accessible profile-creation wizard. With a name or any option, uses the non-interactive form.",
        after_help = "Examples:\n  git explain profile add local \\\n    --preset llama-cpp \\\n    --model qwen3.5-35b\n\n  git explain profile add ollama \\\n    --preset ollama \\\n    --model ministral-3:8b\n\n  git explain profile add cloud \\\n    --provider openai-compatible \\\n    --base-url https://example.com/v1 \\\n    --model model-name \\\n    --api-key-env CLOUD_API_KEY"
    )]
    Add {
        name: Option<String>,
        #[arg(long, help = "Model API protocol. Default: openai-compatible")]
        provider: Option<String>,
        #[arg(
            short = 's',
            long,
            help = "Apply defaults for a known local model server. Supported: llama-cpp, ollama. Example: --preset llama-cpp --model-port 9000"
        )]
        preset: Option<String>,
        #[arg(
            short = 'u',
            long,
            help = "Use a complete custom model endpoint. Cannot be used with --model-port"
        )]
        base_url: Option<String>,
        #[arg(
            long,
            help = "Override only the port of the selected preset's model endpoint"
        )]
        model_port: Option<u16>,
        #[arg(short = 'm', long, help = "Model name or identifier")]
        model: Option<String>,
        #[arg(long, help = "Environment variable containing the API key")]
        api_key_env: Option<String>,
        #[arg(
            long,
            help = "Optional git-explain context budget cap; does not configure the model server"
        )]
        context_window: Option<u32>,
        #[arg(
            long,
            help = "Whether normal responses should use reasoning (true or false)"
        )]
        normal_reasoning: Option<bool>,
        #[arg(long, help = "Maximum tokens for normal responses")]
        normal_max_tokens: Option<u32>,
        #[arg(long, help = "Temperature for normal responses")]
        normal_temperature: Option<f32>,
        #[arg(
            long,
            help = "Whether deep responses should use reasoning (true or false)"
        )]
        deep_reasoning: Option<bool>,
        #[arg(long, help = "Maximum tokens for deep responses")]
        deep_max_tokens: Option<u32>,
        #[arg(long, help = "Temperature for deep responses")]
        deep_temperature: Option<f32>,
    },
    #[command(
        about = "Update selected fields of a trusted user profile",
        after_help = "Examples:\n  git explain profile edit local\n  git explain profile edit local --model qwen3.5-35b\n  git explain profile edit local --model-port 9000\n  git explain profile edit cloud --base-url https://example.com/v1\n\nWith no edit options, starts an interactive editor when standard input is a terminal. With edit options, applies changes non-interactively.\n\n--model-port changes only the model endpoint port. --base-url replaces the complete endpoint.\n\nUse --clear-preset, --clear-api-key-env, or a --clear-<mode>-<field> flag to remove an optional value."
    )]
    Edit {
        name: String,
        #[arg(long, help = "Model API protocol")]
        provider: Option<String>,
        #[arg(short = 's', long, help = "Change the profile preset")]
        preset: Option<String>,
        #[arg(short = 'u', long, help = "Replace the complete model endpoint URL")]
        base_url: Option<String>,
        #[arg(
            long,
            help = "Change only the port of the existing preset model endpoint"
        )]
        model_port: Option<u16>,
        #[arg(short = 'm', long, help = "Change the model name")]
        model: Option<String>,
        #[arg(long, help = "Set the API-key environment variable name")]
        api_key_env: Option<String>,
        #[arg(
            long,
            conflicts_with = "clear_context_window",
            help = "Set a git-explain context budget cap; does not configure the model server"
        )]
        context_window: Option<u32>,
        #[arg(long)]
        clear_preset: bool,
        #[arg(long)]
        clear_api_key_env: bool,
        #[arg(long, conflicts_with = "context_window")]
        clear_context_window: bool,
        #[arg(long, conflicts_with = "clear_normal_reasoning")]
        normal_reasoning: Option<bool>,
        #[arg(long, conflicts_with = "clear_normal_max_tokens")]
        normal_max_tokens: Option<u32>,
        #[arg(long, conflicts_with = "clear_normal_temperature")]
        normal_temperature: Option<f32>,
        #[arg(long, conflicts_with = "clear_deep_reasoning")]
        deep_reasoning: Option<bool>,
        #[arg(long, conflicts_with = "clear_deep_max_tokens")]
        deep_max_tokens: Option<u32>,
        #[arg(long, conflicts_with = "clear_deep_temperature")]
        deep_temperature: Option<f32>,
        #[arg(long, conflicts_with = "normal_reasoning")]
        clear_normal_reasoning: bool,
        #[arg(long, conflicts_with = "normal_max_tokens")]
        clear_normal_max_tokens: bool,
        #[arg(long, conflicts_with = "normal_temperature")]
        clear_normal_temperature: bool,
        #[arg(long, conflicts_with = "deep_reasoning")]
        clear_deep_reasoning: bool,
        #[arg(long, conflicts_with = "deep_max_tokens")]
        clear_deep_max_tokens: bool,
        #[arg(long, conflicts_with = "deep_temperature")]
        clear_deep_temperature: bool,
    },
    #[command(about = "Remove a trusted user profile")]
    Remove { name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_port_is_parsed_as_a_profile_option() {
        let cli = Cli::try_parse_from([
            "git-explain",
            "profile",
            "add",
            "local",
            "--preset",
            "llama-cpp",
            "--model-port",
            "9000",
            "--model",
            "foo",
        ])
        .unwrap();
        let Command::Profile(ProfileCommand {
            action: ProfileAction::Add { model_port, .. },
        }) = cli.command.unwrap()
        else {
            panic!("expected profile add");
        };
        assert_eq!(model_port, Some(9000));
    }

    #[test]
    fn context_window_is_a_profile_option() {
        let cli = Cli::try_parse_from([
            "git-explain",
            "profile",
            "edit",
            "local",
            "--context-window",
            "32768",
        ])
        .unwrap();
        let Command::Profile(ProfileCommand {
            action: ProfileAction::Edit { context_window, .. },
        }) = cli.command.unwrap()
        else {
            panic!("expected profile edit");
        };
        assert_eq!(context_window, Some(32_768));
    }

    #[test]
    fn model_port_rejects_values_above_u16_range() {
        let result = Cli::try_parse_from([
            "git-explain",
            "profile",
            "add",
            "local",
            "--preset",
            "llama-cpp",
            "--model-port",
            "65536",
            "--model",
            "foo",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn top_level_aliases_parse_to_canonical_commands() {
        let canonical = Cli::try_parse_from(["git-explain", "profile", "list"]).unwrap();
        let alias = Cli::try_parse_from(["git-explain", "prof", "list"]).unwrap();
        assert!(matches!(
            canonical.command,
            Some(Command::Profile(ProfileCommand {
                action: ProfileAction::List
            }))
        ));
        assert!(matches!(
            alias.command,
            Some(Command::Profile(ProfileCommand {
                action: ProfileAction::List
            }))
        ));

        let canonical = Cli::try_parse_from(["git-explain", "config", "show"]).unwrap();
        let alias = Cli::try_parse_from(["git-explain", "cfg", "show"]).unwrap();
        assert!(matches!(
            canonical.command,
            Some(Command::Config(ConfigCommand {
                action: ConfigAction::Show
            }))
        ));
        assert!(matches!(
            alias.command,
            Some(Command::Config(ConfigCommand {
                action: ConfigAction::Show
            }))
        ));
    }

    #[test]
    fn profile_short_options_parse_into_canonical_fields() {
        let cli = Cli::try_parse_from([
            "git-explain",
            "prof",
            "add",
            "local",
            "-s",
            "llama-cpp",
            "-u",
            "http://localhost:9000/v1",
            "-m",
            "qwen",
        ])
        .unwrap();
        let Command::Profile(ProfileCommand {
            action:
                ProfileAction::Add {
                    preset,
                    base_url,
                    model,
                    ..
                },
        }) = cli.command.unwrap()
        else {
            panic!("expected profile add");
        };
        assert_eq!(preset.as_deref(), Some("llama-cpp"));
        assert_eq!(base_url.as_deref(), Some("http://localhost:9000/v1"));
        assert_eq!(model.as_deref(), Some("qwen"));

        let cli =
            Cli::try_parse_from(["git-explain", "profile", "edit", "local", "-m", "new"]).unwrap();
        let Command::Profile(ProfileCommand {
            action: ProfileAction::Edit { model, .. },
        }) = cli.command.unwrap()
        else {
            panic!("expected profile edit");
        };
        assert_eq!(model.as_deref(), Some("new"));
    }

    #[test]
    fn profile_add_accepts_all_generation_fields() {
        let cli = Cli::try_parse_from([
            "git-explain",
            "profile",
            "add",
            "local",
            "--provider",
            "openai-compatible",
            "--preset",
            "llama-cpp",
            "--model-port",
            "9000",
            "--model",
            "qwen",
            "--api-key-env",
            "MODEL_KEY",
            "--normal-reasoning",
            "false",
            "--normal-max-tokens",
            "600",
            "--normal-temperature",
            "0.2",
            "--deep-reasoning",
            "true",
            "--deep-max-tokens",
            "3000",
            "--deep-temperature",
            "0.35",
        ])
        .unwrap();
        let Command::Profile(ProfileCommand {
            action:
                ProfileAction::Add {
                    normal_reasoning,
                    normal_max_tokens,
                    normal_temperature,
                    deep_reasoning,
                    deep_max_tokens,
                    deep_temperature,
                    ..
                },
        }) = cli.command.unwrap()
        else {
            panic!("expected profile add");
        };
        assert_eq!(normal_reasoning, Some(false));
        assert_eq!(normal_max_tokens, Some(600));
        assert_eq!(normal_temperature, Some(0.2));
        assert_eq!(deep_reasoning, Some(true));
        assert_eq!(deep_max_tokens, Some(3000));
        assert_eq!(deep_temperature, Some(0.35));
    }

    #[test]
    fn profile_edit_rejects_generation_set_clear_conflicts() {
        let result = Cli::try_parse_from([
            "git-explain",
            "profile",
            "edit",
            "local",
            "--normal-temperature",
            "0.2",
            "--clear-normal-temperature",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn repo_force_and_debug_short_options_parse() {
        let cli = Cli::try_parse_from(["git-explain", "-d"]).unwrap();
        assert!(cli.debug);

        let cli = Cli::try_parse_from(["git-explain", "cfg", "init", "-r", "-f"]).unwrap();
        let Command::Config(ConfigCommand {
            action: ConfigAction::Init { repo, force },
        }) = cli.command.unwrap()
        else {
            panic!("expected config init");
        };
        assert!(repo && force);

        let cli = Cli::try_parse_from(["git-explain", "prof", "use", "work", "-r"]).unwrap();
        let Command::Profile(ProfileCommand {
            action: ProfileAction::Use { repo, .. },
        }) = cli.command.unwrap()
        else {
            panic!("expected profile use");
        };
        assert!(repo);
    }

    #[test]
    fn port_options_remain_long_only() {
        assert!(Cli::try_parse_from(["git-explain", "-p", "9000"]).is_err());
        assert!(Cli::try_parse_from([
            "git-explain",
            "profile",
            "add",
            "local",
            "--preset",
            "llama-cpp",
            "-p",
            "9000",
            "--model",
            "qwen",
        ])
        .is_err());
    }

    #[test]
    fn application_config_sections_accept_all_editable_fields() {
        for args in [
            vec![
                "git-explain",
                "cfg",
                "reader",
                "--experience",
                "intermediate",
                "--add-known-language",
                "Rust",
                "--remove-learning-language",
                "Java",
                "--clear-known-frameworks",
            ],
            vec![
                "git-explain",
                "config",
                "explanation",
                "--depth",
                "deep",
                "--annotation-limit",
                "20",
                "--annotation-word-limit",
                "80",
                "--explain-language-concepts",
                "false",
                "--explain-framework-concepts",
                "true",
                "--infer-intent",
                "true",
            ],
            vec!["git-explain", "config", "cache", "--enabled", "false"],
            vec![
                "git-explain",
                "config",
                "server",
                "--host",
                "127.0.0.1",
                "--port",
                "9000",
                "--open-browser",
                "false",
            ],
            vec![
                "git-explain",
                "config",
                "git",
                "--diff-target",
                "HEAD~1",
                "--include-staged",
                "false",
                "--include-untracked",
                "true",
                "--repo",
            ],
            vec!["git-explain", "config", "model", "--profile", "local"],
        ] {
            assert!(Cli::try_parse_from(args).is_ok());
        }
    }
}
