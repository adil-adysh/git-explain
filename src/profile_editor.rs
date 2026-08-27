use crate::config::{
    add_profile_with_update, display_preset, display_provider, edit_profile, preview_profile,
    profile_preset, profile_presets, ProfileDraft, ProfileUpdate, ResolvedProfile,
};
use crate::terminal::{confirmation, read_line as line};
use anyhow::{Context, Result};
use std::io::{BufRead, Write};
use std::path::Path;

pub fn run<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    path: &Path,
    name: &str,
    current: &ResolvedProfile,
) -> Result<()> {
    let mut draft = ProfileUpdate::default();
    writeln!(output, "Editing profile: {name}")?;
    loop {
        let preview = preview_profile(path, name, &draft).unwrap_or_else(|_| current.clone());
        write_summary(output, &preview)?;
        writeln!(output, "\nWhat do you want to change?\n")?;
        writeln!(output, "1. Provider")?;
        writeln!(output, "2. Model")?;
        writeln!(output, "3. Model endpoint")?;
        writeln!(output, "4. Preset")?;
        writeln!(output, "5. API key environment variable")?;
        writeln!(output, "6. Normal generation settings")?;
        writeln!(output, "7. Deep generation settings")?;
        writeln!(output, "8. Review changes and save")?;
        writeln!(output, "9. Cancel")?;
        let choice = choice(input, output, "Enter 1-9:", 1, 9)?;
        match choice {
            1 => edit_provider(input, output, &mut draft, &preview)?,
            2 => edit_model(input, output, &mut draft, &preview)?,
            3 => edit_endpoint(input, output, &mut draft, &preview)?,
            4 => edit_preset(input, output, &mut draft, &preview)?,
            5 => edit_api_key_env(input, output, &mut draft, &preview)?,
            6 => edit_generation(input, output, &mut draft, &preview, false)?,
            7 => edit_generation(input, output, &mut draft, &preview, true)?,
            8 => {
                if !draft.has_changes() {
                    writeln!(output, "No profile changes were made.")?;
                    return Ok(());
                }
                let resulting = match preview_profile(path, name, &draft) {
                    Ok(profile) => profile,
                    Err(error) => {
                        writeln!(output, "The profile cannot be saved because the resulting configuration is invalid.\n\nReason:\n{error}\n\nNo configuration was changed.")?;
                        continue;
                    }
                };
                write_review(output, name, current, &resulting)?;
                if confirmation(input, output, "Save these changes? [y/N]:")? {
                    edit_profile(path, name, draft)?;
                    writeln!(output, "Updated profile: {name}\n\nBase URL: {}\nModel: {}\n\nTest the profile:\n\ngit explain profile test {name}", resulting.base_url, resulting.model)?;
                    return Ok(());
                }
            }
            9 => {
                writeln!(
                    output,
                    "Profile edit cancelled.\n\nNo configuration was changed."
                )?;
                return Ok(());
            }
            _ => unreachable!(),
        }
    }
}

pub fn run_add<R: BufRead, W: Write>(input: &mut R, output: &mut W, path: &Path) -> Result<()> {
    writeln!(output, "Create a new model profile")?;
    let name = required_line(input, output, "Profile name (or `cancel`):")?;
    if name.eq_ignore_ascii_case("cancel") {
        writeln!(
            output,
            "Profile creation cancelled.\n\nNo configuration was changed."
        )?;
        return Ok(());
    }

    writeln!(
        output,
        "\nChoose a provider:\n\n1. OpenAI-compatible\n2. Cancel"
    )?;
    if choice(input, output, "Enter 1-2:", 1, 2)? == 2 {
        writeln!(
            output,
            "Profile creation cancelled.\n\nNo configuration was changed."
        )?;
        return Ok(());
    }
    writeln!(
        output,
        "\nChoose a model endpoint:\n\n1. {} ({})\n2. {} ({})\n3. Custom OpenAI-compatible URL\n4. Cancel",
        profile_presets()[0].display_name,
        profile_presets()[0].default_base_url.unwrap_or("<none>"),
        profile_presets()[1].display_name,
        profile_presets()[1].default_base_url.unwrap_or("<none>")
    )?;
    let endpoint_choice = choice(input, output, "Enter 1-4:", 1, 4)?;
    if endpoint_choice == 4 {
        writeln!(
            output,
            "Profile creation cancelled.\n\nNo configuration was changed."
        )?;
        return Ok(());
    }
    let (preset, base_url) = match endpoint_choice {
        1 => (Some("llama_cpp".to_owned()), None),
        2 => (Some("ollama".to_owned()), None),
        3 => {
            let url = loop {
                let value = required_line(input, output, "Base URL (or `cancel`):")?;
                if value.eq_ignore_ascii_case("cancel") {
                    writeln!(
                        output,
                        "Profile creation cancelled.\n\nNo configuration was changed."
                    )?;
                    return Ok(());
                }
                if valid_url(&value) {
                    break value;
                }
                writeln!(output, "Invalid base URL. Enter a valid HTTP or HTTPS URL:")?;
            };
            (None, Some(url))
        }
        _ => unreachable!(),
    };

    let model = required_line(input, output, "Model name (or `cancel`):")?;
    if model.eq_ignore_ascii_case("cancel") {
        writeln!(
            output,
            "Profile creation cancelled.\n\nNo configuration was changed."
        )?;
        return Ok(());
    }
    writeln!(
        output,
        "API-key environment variable (optional; press Enter to skip):"
    )?;
    let api_key_env = line(input)?.filter(|value| !value.is_empty());
    let mut update = ProfileUpdate::default();
    writeln!(output, "\nConfigure advanced generation settings? [y/N]:")?;
    if matches!(line(input)?.as_deref(), Some("y" | "yes" | "Y" | "YES")) {
        let draft = ResolvedProfile {
            provider: "openai_compatible".into(),
            preset: profile_preset_or_none(preset.as_deref()),
            base_url: base_url
                .clone()
                .or_else(|| {
                    preset
                        .as_deref()
                        .and_then(profile_preset)
                        .and_then(|p| p.default_base_url)
                        .map(str::to_owned)
                })
                .unwrap_or_default(),
            model: model.clone(),
            api_key_env: api_key_env.clone(),
            api_key: None,
            normal: crate::config::GenerationConfig {
                reasoning: None,
                max_tokens: None,
                temperature: None,
            },
            deep: crate::config::GenerationConfig {
                reasoning: None,
                max_tokens: None,
                temperature: None,
            },
        };
        edit_generation(input, output, &mut update, &draft, false)?;
        edit_generation(input, output, &mut update, &draft, true)?;
    }
    let profile = ProfileDraft {
        name: name.clone(),
        provider: Some("openai_compatible".into()),
        preset,
        base_url,
        model_port: None,
        model: model.clone(),
        api_key_env,
    };
    writeln!(
        output,
        "\nProfile: {name}\n\nPreset: {}\nBase URL: {}\nModel: {}\nAPI key environment variable: {}",
        profile
            .preset
            .as_deref()
            .map(display_preset)
            .unwrap_or("none"),
        profile.base_url.as_deref().unwrap_or("provided by preset"),
        profile.model,
        profile.api_key_env.as_deref().unwrap_or("none")
    )?;
    if confirmation(input, output, "Create this profile? [y/N]:")? {
        add_profile_with_update(path, profile, update)?;
        writeln!(
            output,
            "Profile created: {name}\n\nNext:\ngit explain profile test {name}"
        )?;
    } else {
        writeln!(
            output,
            "Profile creation cancelled.\n\nNo configuration was changed."
        )?;
    }
    Ok(())
}

fn profile_preset_or_none(value: Option<&str>) -> Option<String> {
    value.map(str::to_owned)
}

fn edit_provider<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    draft: &mut ProfileUpdate,
    profile: &ResolvedProfile,
) -> Result<()> {
    writeln!(
        output,
        "\nCurrent provider:\n{}\n\nAvailable providers:\n\n1. OpenAI-compatible\n2. Cancel",
        display_provider(&profile.provider)
    )?;
    if choice(input, output, "Enter 1-2:", 1, 2)? == 1 {
        draft.provider = Some("openai_compatible".into());
    }
    Ok(())
}

fn required_line<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
) -> Result<String> {
    loop {
        writeln!(output, "\n{prompt}")?;
        match line(input)? {
            Some(value) if !value.is_empty() => return Ok(value),
            Some(_) => writeln!(output, "A value is required:")?,
            None => anyhow::bail!("Profile creation cancelled.\n\nNo configuration was changed."),
        }
    }
}

fn write_summary<W: Write>(output: &mut W, profile: &ResolvedProfile) -> Result<()> {
    writeln!(
        output,
        "\nProvider: {}\nPreset: {}\nBase URL: {}\nModel: {}\nAPI key environment variable: {}",
        display_provider(&profile.provider),
        profile
            .preset
            .as_deref()
            .map(display_preset)
            .unwrap_or("none"),
        profile.base_url,
        profile.model,
        profile.api_key_env.as_deref().unwrap_or("none")
    )?;
    write_generation(output, "Normal generation", &profile.normal)?;
    write_generation(output, "Deep generation", &profile.deep)
}

fn write_generation<W: Write>(
    output: &mut W,
    title: &str,
    generation: &crate::config::GenerationConfig,
) -> Result<()> {
    writeln!(
        output,
        "\n{title}:\n  Reasoning: {}\n  Max tokens: {}\n  Temperature: {}",
        optional(generation.reasoning),
        optional(generation.max_tokens),
        optional(generation.temperature)
    )?;
    Ok(())
}

fn optional<T: std::fmt::Display>(value: Option<T>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unspecified".into())
}

fn edit_model<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    draft: &mut ProfileUpdate,
    profile: &ResolvedProfile,
) -> Result<()> {
    writeln!(output, "\nCurrent model:\n{}\n\nNew model:", profile.model)?;
    if let Some(value) = line(input)? {
        if !value.is_empty() && !value.eq_ignore_ascii_case("cancel") {
            writeln!(output, "Model will change to:\n{value}")?;
            draft.model = Some(value);
        }
    }
    Ok(())
}

fn edit_endpoint<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    draft: &mut ProfileUpdate,
    profile: &ResolvedProfile,
) -> Result<()> {
    writeln!(output, "\nCurrent endpoint:\n{}", profile.base_url)?;
    let has_preset = profile.preset.as_deref().and_then(profile_preset).is_some();
    let max = if has_preset { 4 } else { 2 };
    if has_preset {
        writeln!(output, "\nHow do you want to change it?\n\n1. Change only the model port\n2. Enter a complete base URL\n3. Restore the preset default endpoint\n4. Keep the current endpoint")?;
    } else {
        writeln!(output, "\nHow do you want to change it?\n\n1. Enter a complete base URL\n2. Keep the current endpoint")?;
    }
    let selected = choice(input, output, &format!("Enter 1-{max}:"), 1, max)?;
    match (has_preset, selected) {
        (true, 1) => {
            let current_port = reqwest::Url::parse(&profile.base_url)
                .ok()
                .and_then(|url| url.port())
                .map(|port| port.to_string())
                .unwrap_or_default();
            writeln!(
                output,
                "\nCurrent model port:\n{}\n\nNew model port:",
                current_port
            )?;
            if let Some(value) = line(input)? {
                let port = parse_port(&value, output)?;
                if let Some(port) = port {
                    draft.model_port = Some(port);
                    draft.base_url = None;
                }
            }
        }
        (true, 2) | (false, 1) => {
            writeln!(
                output,
                "\nCurrent base URL:\n{}\n\nNew base URL:",
                profile.base_url
            )?;
            if let Some(value) = line(input)? {
                if value.eq_ignore_ascii_case("cancel") {
                    return Ok(());
                }
                if valid_url(&value) {
                    draft.base_url = Some(value);
                    draft.model_port = None;
                } else {
                    writeln!(output, "Invalid base URL: {value}\nEnter a valid HTTP or HTTPS URL, or type `cancel` to return.")?;
                }
            }
        }
        (true, 3) => {
            let preset = profile
                .preset
                .as_deref()
                .and_then(profile_preset)
                .context("preset endpoint is unavailable")?;
            draft.base_url = preset.default_base_url.map(str::to_owned);
            draft.model_port = None;
        }
        _ => {}
    }
    Ok(())
}

fn edit_preset<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    draft: &mut ProfileUpdate,
    profile: &ResolvedProfile,
) -> Result<()> {
    writeln!(
        output,
        "\nCurrent preset:\n{}\n\nAvailable presets:\n\n1. {}\n2. {}\n3. No preset\n4. Cancel",
        profile
            .preset
            .as_deref()
            .map(display_preset)
            .unwrap_or("none"),
        profile_presets()[0].display_name,
        profile_presets()[1].display_name
    )?;
    match choice(input, output, "Enter 1-4:", 1, 4)? {
        selected @ (1 | 2) => {
            let id = if selected == 1 { "llama_cpp" } else { "ollama" };
            let preset = profile_preset(id).context("preset endpoint is unavailable")?;
            draft.preset = Some(id.into());
            draft.clear_preset = false;
            if profile.base_url != preset.default_base_url.unwrap_or("") {
                writeln!(output, "\nThe current endpoint is:\n\n{}\n\nThe {} preset default is:\n\n{}\n\nKeep the current endpoint?\n\n1. Keep current endpoint\n2. Use preset default endpoint", profile.base_url, preset.display_name, preset.default_base_url.unwrap_or("<none>"))?;
                if choice(input, output, "Enter 1-2:", 1, 2)? == 2 {
                    draft.base_url = preset.default_base_url.map(str::to_owned);
                    draft.model_port = None;
                }
            }
        }
        3 => {
            draft.preset = None;
            draft.clear_preset = true;
        }
        _ => {}
    }
    Ok(())
}

fn edit_api_key_env<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    draft: &mut ProfileUpdate,
    profile: &ResolvedProfile,
) -> Result<()> {
    writeln!(output, "\nCurrent API key environment variable:\n{}\n\n1. Set an environment variable name\n2. Clear the environment variable\n3. Keep unchanged", profile.api_key_env.as_deref().unwrap_or("none"))?;
    match choice(input, output, "Enter 1-3:", 1, 3)? {
        1 => {
            writeln!(output, "Environment variable name:")?;
            if let Some(value) = line(input)? {
                if !value.is_empty() {
                    draft.api_key_env = Some(value);
                    draft.clear_api_key_env = false;
                }
            }
        }
        2 => {
            draft.api_key_env = None;
            draft.clear_api_key_env = true;
        }
        _ => {}
    }
    Ok(())
}

fn edit_generation<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    draft: &mut ProfileUpdate,
    profile: &ResolvedProfile,
    deep: bool,
) -> Result<()> {
    let title = if deep { "Deep" } else { "Normal" };
    let generation = if deep { &profile.deep } else { &profile.normal };
    loop {
        writeln!(output, "\n{title} generation settings:\n\n1. Reasoning: {}\n2. Max tokens: {}\n3. Temperature: {}\n4. Return", optional(generation.reasoning), optional(generation.max_tokens), optional(generation.temperature))?;
        match choice(input, output, "Enter 1-4:", 1, 4)? {
            1 => edit_reasoning(input, output, draft, deep)?,
            2 => edit_max_tokens(input, output, draft, deep)?,
            3 => edit_temperature(input, output, draft, deep)?,
            4 => return Ok(()),
            _ => unreachable!(),
        }
    }
}

fn edit_reasoning<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    draft: &mut ProfileUpdate,
    deep: bool,
) -> Result<()> {
    writeln!(
        output,
        "\n1. Enabled\n2. Disabled\n3. Unspecified\n4. Cancel"
    )?;
    match choice(input, output, "Enter 1-4:", 1, 4)? {
        1 => set_reasoning(draft, deep, Some(true)),
        2 => set_reasoning(draft, deep, Some(false)),
        3 => set_reasoning(draft, deep, None),
        _ => {}
    }
    Ok(())
}

fn set_reasoning(draft: &mut ProfileUpdate, deep: bool, value: Option<bool>) {
    if deep {
        draft.deep_reasoning = value;
        draft.clear_deep_reasoning = value.is_none();
    } else {
        draft.normal_reasoning = value;
        draft.clear_normal_reasoning = value.is_none();
    }
}

fn edit_max_tokens<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    draft: &mut ProfileUpdate,
    deep: bool,
) -> Result<()> {
    writeln!(output, "Enter a new value, `clear`, or `cancel`:")?;
    if let Some(value) = line(input)? {
        if value.eq_ignore_ascii_case("clear") {
            if deep {
                draft.clear_deep_max_tokens = true;
                draft.deep_max_tokens = None;
            } else {
                draft.clear_normal_max_tokens = true;
                draft.normal_max_tokens = None;
            }
        } else if !value.eq_ignore_ascii_case("cancel") {
            match value.parse::<u32>() {
                Ok(value) if value > 0 => {
                    if deep {
                        draft.deep_max_tokens = Some(value);
                        draft.clear_deep_max_tokens = false;
                    } else {
                        draft.normal_max_tokens = Some(value);
                        draft.clear_normal_max_tokens = false;
                    }
                }
                _ => writeln!(
                    output,
                    "Invalid max tokens. Enter a positive integer, `clear`, or `cancel`."
                )?,
            }
        }
    }
    Ok(())
}

fn edit_temperature<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    draft: &mut ProfileUpdate,
    deep: bool,
) -> Result<()> {
    writeln!(output, "Enter a new value, `clear`, or `cancel`:")?;
    if let Some(value) = line(input)? {
        if value.eq_ignore_ascii_case("clear") {
            if deep {
                draft.clear_deep_temperature = true;
                draft.deep_temperature = None;
            } else {
                draft.clear_normal_temperature = true;
                draft.normal_temperature = None;
            }
        } else if !value.eq_ignore_ascii_case("cancel") {
            match value.parse::<f32>() {
                Ok(value) if value.is_finite() && (0.0..=2.0).contains(&value) => {
                    if deep {
                        draft.deep_temperature = Some(value);
                        draft.clear_deep_temperature = false;
                    } else {
                        draft.normal_temperature = Some(value);
                        draft.clear_normal_temperature = false;
                    }
                }
                _ => writeln!(
                    output,
                    "Invalid temperature. Enter a finite value from 0 to 2, `clear`, or `cancel`."
                )?,
            }
        }
    }
    Ok(())
}

fn write_review<W: Write>(
    output: &mut W,
    name: &str,
    old: &ResolvedProfile,
    new: &ResolvedProfile,
) -> Result<()> {
    writeln!(output, "\nProfile: {name}\n\nProvider: {}\nPreset: {}\nBase URL: {}\nModel: {}\n\nAPI key environment variable: {}", display_provider(&new.provider), new.preset.as_deref().map(display_preset).unwrap_or("none"), new.base_url, new.model, new.api_key_env.as_deref().unwrap_or("none"))?;
    write_generation(output, "Normal", &new.normal)?;
    write_generation(output, "Deep", &new.deep)?;
    writeln!(output, "\nChanges:")?;
    if old.provider != new.provider {
        writeln!(
            output,
            "  Provider: {} -> {}",
            display_provider(&old.provider),
            display_provider(&new.provider)
        )?;
    }
    if old.preset != new.preset {
        writeln!(
            output,
            "  Preset: {} -> {}",
            old.preset
                .as_deref()
                .map(display_preset)
                .unwrap_or("unspecified"),
            new.preset
                .as_deref()
                .map(display_preset)
                .unwrap_or("unspecified")
        )?;
    }
    if old.model != new.model {
        writeln!(output, "  Model: {} -> {}", old.model, new.model)?;
    }
    if old.base_url != new.base_url {
        writeln!(output, "  Base URL: {} -> {}", old.base_url, new.base_url)?;
    }
    if old.api_key_env != new.api_key_env {
        writeln!(
            output,
            "  API key environment variable: {} -> {}",
            old.api_key_env.as_deref().unwrap_or("unspecified"),
            new.api_key_env.as_deref().unwrap_or("unspecified")
        )?;
    }
    write_generation_changes(output, "Normal", &old.normal, &new.normal)?;
    write_generation_changes(output, "Deep", &old.deep, &new.deep)?;
    Ok(())
}

fn write_generation_changes<W: Write>(
    output: &mut W,
    title: &str,
    old: &crate::config::GenerationConfig,
    new: &crate::config::GenerationConfig,
) -> Result<()> {
    if old.reasoning != new.reasoning {
        writeln!(
            output,
            "  {title} reasoning: {} -> {}",
            optional(old.reasoning),
            optional(new.reasoning)
        )?;
    }
    if old.max_tokens != new.max_tokens {
        writeln!(
            output,
            "  {title} max tokens: {} -> {}",
            optional(old.max_tokens),
            optional(new.max_tokens)
        )?;
    }
    if old.temperature != new.temperature {
        writeln!(
            output,
            "  {title} temperature: {} -> {}",
            optional(old.temperature),
            optional(new.temperature)
        )?;
    }
    Ok(())
}

fn choice<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
    min: u32,
    max: u32,
) -> Result<u32> {
    loop {
        writeln!(output, "\n{prompt}")?;
        match line(input)? {
            Some(value) => match value.parse::<u32>() {
                Ok(value) if (min..=max).contains(&value) => return Ok(value),
                _ => writeln!(
                    output,
                    "Invalid choice. Enter a number from {min} to {max}:"
                )?,
            },
            None => anyhow::bail!("Profile edit cancelled.\n\nNo configuration was changed."),
        }
    }
}

fn parse_port<W: Write>(value: &str, output: &mut W) -> Result<Option<u16>> {
    match value.parse::<u16>() {
        Ok(0) | Err(_) => {
            writeln!(
                output,
                "Invalid model port. Enter a number from 1 to 65535:"
            )?;
            Ok(None)
        }
        Ok(port) => Ok(Some(port)),
    }
}
fn valid_url(value: &str) -> bool {
    reqwest::Url::parse(value)
        .map(|url| matches!(url.scheme(), "http" | "https") && url.host().is_some())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{add_profile, ProfileDraft};
    use std::fs;
    use std::io::Cursor;
    use tempfile::tempdir;

    #[test]
    fn injected_input_edits_model_and_port_before_saving() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        add_profile(
            &path,
            ProfileDraft {
                name: "local".into(),
                provider: None,
                preset: Some("llama-cpp".into()),
                base_url: None,
                model_port: None,
                model: "old".into(),
                api_key_env: None,
            },
        )
        .unwrap();
        let current = preview_profile(&path, "local", &ProfileUpdate::default()).unwrap();
        let mut input = Cursor::new("2\nnew-model\n3\n1\n9000\n8\ny\n");
        let mut output = Vec::new();
        run(&mut input, &mut output, &path, "local", &current).unwrap();
        let resulting = preview_profile(&path, "local", &ProfileUpdate::default()).unwrap();
        assert_eq!(resulting.model, "new-model");
        assert_eq!(resulting.base_url, "http://127.0.0.1:9000/v1");
        assert!(String::from_utf8(output)
            .unwrap()
            .contains("Updated profile: local"));
        assert!(fs::read_to_string(path).unwrap().contains("new-model"));
    }

    #[test]
    fn cancel_leaves_configuration_unchanged() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        add_profile(
            &path,
            ProfileDraft {
                name: "local".into(),
                provider: None,
                preset: Some("llama-cpp".into()),
                base_url: None,
                model_port: None,
                model: "old".into(),
                api_key_env: None,
            },
        )
        .unwrap();
        let original = fs::read_to_string(&path).unwrap();
        let current = preview_profile(&path, "local", &ProfileUpdate::default()).unwrap();
        let mut input = Cursor::new("2\nnew-model\n9\n");
        let mut output = Vec::new();
        run(&mut input, &mut output, &path, "local", &current).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), original);
        assert!(String::from_utf8(output)
            .unwrap()
            .contains("Profile edit cancelled."));
    }

    #[test]
    fn interactive_add_creates_profile_without_secret_values() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut input = Cursor::new("local\n1\n1\nqwen\n\nn\ny\n");
        let mut output = Vec::new();
        run_add(&mut input, &mut output, &path).unwrap();

        let created = crate::config::ConfigLoader::with_paths(path.clone(), None)
            .resolve(Some("local"))
            .unwrap();
        assert_eq!(created.model.model, "qwen");
        assert_eq!(created.model.base_url, "http://127.0.0.1:8083/v1");
        assert!(created.model.api_key.is_none());
        assert!(String::from_utf8(output)
            .unwrap()
            .contains("Profile created: local"));
    }

    #[test]
    fn interactive_add_cancel_does_not_write_configuration() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut input = Cursor::new("cancel\n");
        let mut output = Vec::new();
        run_add(&mut input, &mut output, &path).unwrap();
        assert!(!path.exists());
        assert!(String::from_utf8(output)
            .unwrap()
            .contains("Profile creation cancelled."));
    }

    #[test]
    fn interactive_add_can_configure_both_generation_sections() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut input = Cursor::new(
            "local\n1\n1\nqwen\n\ny\n1\n1\n2\n600\n3\n0.2\n4\n1\n2\n2\n3000\n3\n0.35\n4\ny\n",
        );
        let mut output = Vec::new();
        run_add(&mut input, &mut output, &path).unwrap();
        let created = crate::config::ConfigLoader::with_paths(path, None)
            .resolve(Some("local"))
            .unwrap();
        assert_eq!(created.model.normal.reasoning, Some(true));
        assert_eq!(created.model.normal.max_tokens, Some(600));
        assert_eq!(created.model.normal.temperature, Some(0.2));
        assert_eq!(created.model.deep.reasoning, Some(false));
        assert_eq!(created.model.deep.max_tokens, Some(3000));
        assert_eq!(created.model.deep.temperature, Some(0.35));
    }
}
