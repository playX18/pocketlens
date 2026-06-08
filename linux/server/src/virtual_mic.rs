use std::process::Command;

pub const DEFAULT_SINK_NAME: &str = "acamera_sink";
pub const DEFAULT_SOURCE_NAME: &str = "acamera_microphone";
pub const DEFAULT_SINK_DESCRIPTION: &str = "ACamera Audio Sink";
pub const DEFAULT_SOURCE_DESCRIPTION: &str = "ACamera Microphone";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn new(
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    pub fn command_line(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualMicPlan {
    pub sink_name: String,
    pub source_name: String,
    pub sink_description: String,
    pub source_description: String,
}

impl Default for VirtualMicPlan {
    fn default() -> Self {
        Self {
            sink_name: DEFAULT_SINK_NAME.to_string(),
            source_name: DEFAULT_SOURCE_NAME.to_string(),
            sink_description: DEFAULT_SINK_DESCRIPTION.to_string(),
            source_description: DEFAULT_SOURCE_DESCRIPTION.to_string(),
        }
    }
}

impl VirtualMicPlan {
    pub fn load_commands(&self) -> Vec<CommandSpec> {
        vec![
            CommandSpec::new(
                "pactl",
                [
                    "load-module",
                    "module-null-sink",
                    &format!("sink_name={}", self.sink_name),
                    &format!(
                        "sink_properties=device.description={}",
                        self.sink_description
                    ),
                    "rate=48000",
                    "channels=2",
                ],
            ),
            CommandSpec::new(
                "pactl",
                [
                    "load-module",
                    "module-remap-source",
                    &format!("master={}.monitor", self.sink_name),
                    &format!("source_name={}", self.source_name),
                    &format!(
                        "source_properties=device.description={}",
                        self.source_description
                    ),
                ],
            ),
        ]
    }

    pub fn unload_commands(module_ids: &[String]) -> Vec<CommandSpec> {
        module_ids
            .iter()
            .map(|module_id| CommandSpec::new("pactl", ["unload-module", module_id]))
            .collect()
    }
}

pub fn setup_virtual_microphone(plan: &VirtualMicPlan) -> anyhow::Result<()> {
    if virtual_microphone_source_exists(plan)? {
        return Ok(());
    }
    for command in plan.load_commands() {
        let status = Command::new(&command.program)
            .args(&command.args)
            .status()?;
        if !status.success() {
            anyhow::bail!(
                "virtual microphone command failed: {}",
                command.command_line()
            );
        }
    }
    Ok(())
}

pub fn remove_virtual_microphone(plan: &VirtualMicPlan) -> anyhow::Result<()> {
    let modules = find_virtual_microphone_module_ids(plan)?;
    for command in VirtualMicPlan::unload_commands(&modules) {
        let status = Command::new(&command.program)
            .args(&command.args)
            .status()?;
        if !status.success() {
            anyhow::bail!(
                "virtual microphone command failed: {}",
                command.command_line()
            );
        }
    }
    Ok(())
}

pub fn find_virtual_microphone_module_ids(plan: &VirtualMicPlan) -> anyhow::Result<Vec<String>> {
    let output = Command::new("pactl")
        .args(["list", "short", "modules"])
        .output()?;
    if !output.status.success() {
        anyhow::bail!("failed to list Pulse/PipeWire modules with pactl");
    }
    let stdout = String::from_utf8(output.stdout)?;
    Ok(parse_virtual_microphone_module_ids(&stdout, plan))
}

pub fn parse_virtual_microphone_module_ids(output: &str, plan: &VirtualMicPlan) -> Vec<String> {
    let mut sink_ids = Vec::new();
    let mut source_ids = Vec::new();
    for line in output.lines() {
        let mut fields = line.split_whitespace();
        let Some(module_id) = fields.next() else {
            continue;
        };
        let Some(module_name) = fields.next() else {
            continue;
        };
        let rest = fields.collect::<Vec<_>>().join(" ");
        if module_name == "module-remap-source"
            && rest.contains(&format!("source_name={}", plan.source_name))
        {
            source_ids.push(module_id.to_string());
        } else if module_name == "module-null-sink"
            && rest.contains(&format!("sink_name={}", plan.sink_name))
        {
            sink_ids.push(module_id.to_string());
        }
    }
    source_ids.extend(sink_ids);
    source_ids
}

pub fn virtual_microphone_source_exists(plan: &VirtualMicPlan) -> anyhow::Result<bool> {
    let output = Command::new("pactl")
        .args(["list", "short", "sources"])
        .output()?;
    if !output.status.success() {
        anyhow::bail!("failed to list Pulse/PipeWire sources with pactl");
    }
    let stdout = String::from_utf8(output.stdout)?;
    Ok(parse_source_exists(&stdout, &plan.source_name))
}

pub fn parse_source_exists(output: &str, source_name: &str) -> bool {
    output.lines().any(|line| {
        let mut fields = line.split_whitespace();
        let _index = fields.next();
        fields.next() == Some(source_name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_null_sink_and_remap_source_commands() {
        let plan = VirtualMicPlan::default();

        let commands = plan.load_commands();

        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].args,
            vec![
                "load-module",
                "module-null-sink",
                "sink_name=acamera_sink",
                "sink_properties=device.description=ACamera Audio Sink",
                "rate=48000",
                "channels=2"
            ]
        );
        assert_eq!(
            commands[1].args,
            vec![
                "load-module",
                "module-remap-source",
                "master=acamera_sink.monitor",
                "source_name=acamera_microphone",
                "source_properties=device.description=ACamera Microphone"
            ]
        );
    }

    #[test]
    fn builds_unload_commands_from_module_ids() {
        let commands = VirtualMicPlan::unload_commands(&["41".to_string(), "42".to_string()]);

        assert_eq!(
            commands,
            vec![
                CommandSpec::new("pactl", ["unload-module", "41"]),
                CommandSpec::new("pactl", ["unload-module", "42"])
            ]
        );
    }

    #[test]
    fn parses_virtual_microphone_module_ids() {
        let output = "\
41\tmodule-null-sink\tsink_name=acamera_sink channels=2
42\tmodule-remap-source\tmaster=acamera_sink.monitor source_name=acamera_microphone
43\tmodule-null-sink\tsink_name=other
";

        assert_eq!(
            parse_virtual_microphone_module_ids(output, &VirtualMicPlan::default()),
            vec!["42".to_string(), "41".to_string()]
        );
    }

    #[test]
    fn parses_short_source_listing() {
        let output = "\
12\talsa_input.pci-0000_00_1f.3.analog-stereo\tPipeWire\ts32le 2ch 48000Hz
13\tacamera_microphone\tPipeWire\tfloat32le 2ch 48000Hz
";

        assert!(parse_source_exists(output, DEFAULT_SOURCE_NAME));
        assert!(!parse_source_exists(output, "missing_source"));
    }
}
