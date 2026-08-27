//! Deliberately linear, screen-reader-first configuration editor.
use crate::config::{edit_config, ApplicationConfig, ConfigUpdate, ListUpdate};
use crate::terminal::{confirmation as confirm, read_line as line};
use anyhow::Result;
use std::io::{BufRead, Write};
use std::path::Path;

pub fn run<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    path: &Path,
    repository: bool,
    profiles: &[String],
    current: &ApplicationConfig,
) -> Result<()> {
    let mut draft = ConfigUpdate::default();
    loop {
        writeln!(output, "\n{} configuration editor\n\n1. Reader\n2. Explanation\n3. Cache\n4. Server\n5. Git\n6. Model profile selection\n7. Review changes and save\n8. Cancel", if repository { "Repository" } else { "User" })?;
        match choice(input, output, "Enter 1-8:", 1, 8)? {
            1 => reader(input, output, &mut draft, current)?,
            2 => explanation(input, output, &mut draft, current)?,
            3 => boolean(
                input,
                output,
                &format!("Cache enabled (current: {})", yes_no(current.cache.enabled)),
                &mut draft.cache.enabled,
            )?,
            4 => server(input, output, &mut draft, current)?,
            5 => git(input, output, &mut draft, current)?,
            6 => profile(input, output, &mut draft, profiles, current)?,
            7 => {
                review(output, &draft)?;
                if confirm(input, output, "Save these changes? [y/N]:")? {
                    match edit_config(path, repository, &draft, profiles)? {
                        true => writeln!(output, "Configuration saved.")?,
                        false => writeln!(output, "No configuration changes were necessary.")?,
                    };
                    return Ok(());
                }
            }
            8 => {
                writeln!(
                    output,
                    "Configuration edit cancelled.\n\nNo configuration was changed."
                )?;
                return Ok(());
            }
            _ => unreachable!(),
        }
    }
}
#[allow(dead_code)]
fn run_repository<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    path: &Path,
    profiles: &[String],
    current: &ApplicationConfig,
) -> Result<()> {
    let mut draft = ConfigUpdate::default();
    loop {
        writeln!(output, "\nRepository configuration editor\n\n1. Explanation\n2. Git\n3. Model profile selection\n4. Review changes and save\n5. Cancel")?;
        match choice(input, output, "Enter 1-5:", 1, 5)? {
            1 => explanation(input, output, &mut draft, current)?,
            2 => git(input, output, &mut draft, current)?,
            3 => profile(input, output, &mut draft, profiles, current)?,
            4 => {
                review(output, &draft)?;
                if confirm(input, output, "Save these changes? [y/N]:")? {
                    match edit_config(path, true, &draft, profiles)? {
                        true => writeln!(output, "Configuration saved.")?,
                        false => writeln!(output, "No configuration changes were necessary.")?,
                    };
                    return Ok(());
                }
            }
            5 => {
                writeln!(
                    output,
                    "Configuration edit cancelled.\n\nNo configuration was changed."
                )?;
                return Ok(());
            }
            _ => unreachable!(),
        }
    }
}
fn reader<R: BufRead, W: Write>(
    i: &mut R,
    o: &mut W,
    d: &mut ConfigUpdate,
    current: &ApplicationConfig,
) -> Result<()> {
    loop {
        writeln!(o,"\nReader settings\n\n1. Experience: {}\n2. Known languages: {}\n3. Learning languages: {}\n4. Known frameworks: {}\n5. Learning frameworks: {}\n6. Return", current.reader.experience, display(&current.reader.known_languages), display(&current.reader.learning_languages), display(&current.reader.known_frameworks), display(&current.reader.learning_frameworks))?;
        match choice(i, o, "Enter 1-6:", 1, 6)? {
            1 => string(i, o, "Experience", &mut d.reader.experience)?,
            2 => list(i, o, "Known languages", &mut d.reader.known_languages)?,
            3 => list(i, o, "Learning languages", &mut d.reader.learning_languages)?,
            4 => list(i, o, "Known frameworks", &mut d.reader.known_frameworks)?,
            5 => list(
                i,
                o,
                "Learning frameworks",
                &mut d.reader.learning_frameworks,
            )?,
            6 => return Ok(()),
            _ => unreachable!(),
        }
    }
}
fn list<R: BufRead, W: Write>(i: &mut R, o: &mut W, title: &str, d: &mut ListUpdate) -> Result<()> {
    writeln!(o, "\n{title}\n\n1. Add\n2. Remove\n3. Clear all\n4. Return")?;
    match choice(i, o, "Enter 1-4:", 1, 4)? {
        1 => {
            if let Some(v) = value(i, o, "Value:")? {
                d.add.push(v)
            }
        }
        2 => {
            if let Some(v) = value(i, o, "Value:")? {
                d.remove.push(v)
            }
        }
        3 => d.clear = true,
        _ => {}
    };
    Ok(())
}
fn explanation<R: BufRead, W: Write>(
    i: &mut R,
    o: &mut W,
    d: &mut ConfigUpdate,
    current: &ApplicationConfig,
) -> Result<()> {
    loop {
        writeln!(o,"\nExplanation settings\n\n1. Default depth: {}\n2. Annotation limit: {}\n3. Annotation word limit: {}\n4. Explain language concepts: {}\n5. Explain framework concepts: {}\n6. Infer intent: {}\n7. Return", current.explanation.default_depth, current.explanation.max_annotations, current.explanation.max_annotation_words, yes_no(current.explanation.explain_language_concepts), yes_no(current.explanation.explain_framework_concepts), yes_no(current.explanation.infer_intent))?;
        match choice(i, o, "Enter 1-7:", 1, 7)? {
            1 => string(
                i,
                o,
                "Depth (normal or deep)",
                &mut d.explanation.default_depth,
            )?,
            2 => number(i, o, "Annotation limit", &mut d.explanation.max_annotations)?,
            3 => number(
                i,
                o,
                "Annotation word limit",
                &mut d.explanation.max_annotation_words,
            )?,
            4 => boolean(
                i,
                o,
                "Explain language concepts",
                &mut d.explanation.explain_language_concepts,
            )?,
            5 => boolean(
                i,
                o,
                "Explain framework concepts",
                &mut d.explanation.explain_framework_concepts,
            )?,
            6 => boolean(i, o, "Infer intent", &mut d.explanation.infer_intent)?,
            7 => return Ok(()),
            _ => unreachable!(),
        }
    }
}
fn server<R: BufRead, W: Write>(
    i: &mut R,
    o: &mut W,
    d: &mut ConfigUpdate,
    current: &ApplicationConfig,
) -> Result<()> {
    loop {
        writeln!(
            o,
            "\nServer settings\n\n1. Host: {}\n2. Port: {}\n3. Open browser: {}\n4. Return",
            current.server.host,
            current.server.port,
            yes_no(current.server.open_browser)
        )?;
        match choice(i, o, "Enter 1-4:", 1, 4)? {
            1 => string(i, o, "Host", &mut d.server.host)?,
            2 => {
                if let Some(v) = value(i, o, "Port:")? {
                    match v.parse::<u16>() {
                        Ok(v) if v > 0 => d.server.port = Some(v),
                        _ => writeln!(o, "Invalid port. Enter a number from 1 to 65535.")?,
                    }
                }
            }
            3 => boolean(i, o, "Open browser", &mut d.server.open_browser)?,
            4 => return Ok(()),
            _ => unreachable!(),
        }
    }
}
fn git<R: BufRead, W: Write>(
    i: &mut R,
    o: &mut W,
    d: &mut ConfigUpdate,
    current: &ApplicationConfig,
) -> Result<()> {
    loop {
        writeln!(o,"\nGit settings\n\n1. Diff target: {}\n2. Include staged changes: {}\n3. Include untracked files: {}\n4. Return", current.git.diff_target, yes_no(current.git.include_staged), yes_no(current.git.include_untracked))?;
        match choice(i, o, "Enter 1-4:", 1, 4)? {
            1 => string(i, o, "Diff target", &mut d.git.diff_target)?,
            2 => boolean(i, o, "Include staged changes", &mut d.git.include_staged)?,
            3 => boolean(
                i,
                o,
                "Include untracked files",
                &mut d.git.include_untracked,
            )?,
            4 => return Ok(()),
            _ => unreachable!(),
        }
    }
}
fn profile<R: BufRead, W: Write>(
    i: &mut R,
    o: &mut W,
    d: &mut ConfigUpdate,
    p: &[String],
    current: &ApplicationConfig,
) -> Result<()> {
    writeln!(
        o,
        "\nModel profile selection\n\nCurrent profile:\n{}\n\nAvailable profiles:",
        current.profile.as_deref().unwrap_or("<none>")
    )?;
    for (n, v) in p.iter().enumerate() {
        writeln!(o, "{}. {}", n + 1, v)?
    }
    writeln!(
        o,
        "{}. No default profile\n{}. Return",
        p.len() + 1,
        p.len() + 2
    )?;
    let x = choice(
        i,
        o,
        &format!("Enter 1-{}:", p.len() + 2),
        1,
        (p.len() + 2) as u32,
    )? as usize;
    if x <= p.len() {
        d.model.profile = Some(p[x - 1].clone());
        d.model.clear_profile = false
    } else if x == p.len() + 1 {
        d.model.profile = None;
        d.model.clear_profile = true
    };
    Ok(())
}
fn string<R: BufRead, W: Write>(
    i: &mut R,
    o: &mut W,
    label: &str,
    d: &mut Option<String>,
) -> Result<()> {
    if let Some(v) = value(i, o, &format!("{label}:"))? {
        if !v.is_empty() {
            *d = Some(v)
        }
    };
    Ok(())
}
fn number<R: BufRead, W: Write>(
    i: &mut R,
    o: &mut W,
    label: &str,
    d: &mut Option<u32>,
) -> Result<()> {
    if let Some(v) = value(i, o, &format!("{label}:"))? {
        match v.parse() {
            Ok(v) if v > 0 => *d = Some(v),
            _ => writeln!(o, "Enter a positive integer.")?,
        }
    };
    Ok(())
}
fn boolean<R: BufRead, W: Write>(
    i: &mut R,
    o: &mut W,
    label: &str,
    d: &mut Option<bool>,
) -> Result<()> {
    writeln!(o, "\n{label}\n\n1. Yes\n2. No\n3. Cancel")?;
    match choice(i, o, "Enter 1-3:", 1, 3)? {
        1 => *d = Some(true),
        2 => *d = Some(false),
        _ => {}
    }
    Ok(())
}
fn review<W: Write>(o: &mut W, d: &ConfigUpdate) -> Result<()> {
    writeln!(o, "\nPending configuration changes:")?;
    change(o, "Reader experience", d.reader.experience.as_deref())?;
    list_change(o, "Known languages", &d.reader.known_languages)?;
    list_change(o, "Learning languages", &d.reader.learning_languages)?;
    list_change(o, "Known frameworks", &d.reader.known_frameworks)?;
    list_change(o, "Learning frameworks", &d.reader.learning_frameworks)?;
    change(
        o,
        "Explanation depth",
        d.explanation.default_depth.as_deref(),
    )?;
    number_change(o, "Annotation limit", d.explanation.max_annotations)?;
    number_change(
        o,
        "Annotation word limit",
        d.explanation.max_annotation_words,
    )?;
    bool_change(
        o,
        "Explain language concepts",
        d.explanation.explain_language_concepts,
    )?;
    bool_change(
        o,
        "Explain framework concepts",
        d.explanation.explain_framework_concepts,
    )?;
    bool_change(o, "Infer intent", d.explanation.infer_intent)?;
    bool_change(o, "Cache enabled", d.cache.enabled)?;
    change(o, "Server host", d.server.host.as_deref())?;
    number_change(o, "Server port", d.server.port.map(u32::from))?;
    bool_change(o, "Open browser", d.server.open_browser)?;
    change(o, "Git diff target", d.git.diff_target.as_deref())?;
    bool_change(o, "Include staged", d.git.include_staged)?;
    bool_change(o, "Include untracked", d.git.include_untracked)?;
    change(o, "Selected profile", d.model.profile.as_deref())?;
    if d.model.clear_profile {
        writeln!(o, "  Selected profile: clear")?;
    }
    if !d.has_changes() {
        writeln!(o, "  No changes.")?;
    }
    Ok(())
}
fn change<W: Write>(o: &mut W, label: &str, value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        writeln!(o, "  {label}: -> {value}")?;
    }
    Ok(())
}
fn number_change<W: Write>(o: &mut W, label: &str, value: Option<u32>) -> Result<()> {
    if let Some(value) = value {
        writeln!(o, "  {label}: -> {value}")?;
    }
    Ok(())
}
fn bool_change<W: Write>(o: &mut W, label: &str, value: Option<bool>) -> Result<()> {
    if let Some(value) = value {
        writeln!(o, "  {label}: -> {}", yes_no(value))?;
    }
    Ok(())
}
fn list_change<W: Write>(o: &mut W, label: &str, value: &ListUpdate) -> Result<()> {
    if value.clear {
        writeln!(o, "  {label}: clear")?;
    }
    for item in &value.add {
        writeln!(o, "  {label}: add {item}")?;
    }
    for item in &value.remove {
        writeln!(o, "  {label}: remove {item}")?;
    }
    Ok(())
}
fn display(values: &[String]) -> String {
    if values.is_empty() {
        "<none>".into()
    } else {
        values.join(", ")
    }
}
fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
fn choice<R: BufRead, W: Write>(i: &mut R, o: &mut W, p: &str, min: u32, max: u32) -> Result<u32> {
    loop {
        writeln!(o, "\n{p}")?;
        match line(i)? {
            Some(v) => match v.parse() {
                Ok(v) if (min..=max).contains(&v) => return Ok(v),
                _ => writeln!(o, "Invalid choice. Enter a number from {min} to {max}.")?,
            },
            None => anyhow::bail!("Configuration edit cancelled.\n\nNo configuration was changed."),
        }
    }
}
fn value<R: BufRead, W: Write>(i: &mut R, o: &mut W, p: &str) -> Result<Option<String>> {
    writeln!(o, "{p}")?;
    line(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigLoader;
    use std::fs;
    use std::io::Cursor;
    use tempfile::tempdir;

    fn config(path: &Path) -> ApplicationConfig {
        ConfigLoader::with_paths(path.to_owned(), None)
            .application_config(None)
            .unwrap()
    }

    #[test]
    fn editor_saves_draft_only_after_review() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(&path, "[server]\nport = 8081\n").unwrap();
        let current = config(&path);
        let mut input = Cursor::new("4\n2\n9000\n4\n7\ny\n");
        let mut output = Vec::new();
        run(&mut input, &mut output, &path, false, &[], &current).unwrap();
        assert_eq!(config(&path).server.port, 9000);
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Server settings\n\n1. Host: 127.0.0.1\n2. Port: 8081"));
        assert!(output.contains("Pending configuration changes:"));
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn cancellation_leaves_file_unchanged() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let original = "[cache]\nenabled = true\n";
        fs::write(&path, original).unwrap();
        let current = config(&path);
        let mut input = Cursor::new("3\n2\n8\n");
        let mut output = Vec::new();
        run(&mut input, &mut output, &path, false, &[], &current).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert!(String::from_utf8(output)
            .unwrap()
            .contains("No configuration was changed."));
    }

    #[test]
    fn eof_cancels_without_writing() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let original = "[server]\nport = 8081\n";
        fs::write(&path, original).unwrap();
        let current = config(&path);
        let mut input = Cursor::new("4\n2\n9000\n");
        let mut output = Vec::new();
        assert!(run(&mut input, &mut output, &path, false, &[], &current).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }
}
