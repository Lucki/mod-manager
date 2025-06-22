use std::{
    process::{Child, Command},
    thread,
    time::Duration,
};
use toml::{map::Map, Value};

#[derive(Clone, Debug)]
pub struct ExternalCommand {
    pub id: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    wait_for_exit: bool,
    delay_after: Option<u64>,
}

impl ExternalCommand {
    pub fn new(
        id: String,
        args: Vec<String>,
        wait_for_exit: Option<bool>,
        delay_after: Option<u64>,
    ) -> Self {
        ExternalCommand {
            id,
            args,
            env: vec![],
            wait_for_exit: wait_for_exit.unwrap_or(false),
            delay_after,
        }
    }

    pub fn from_config(
        game_name: String,
        id: String,
        config: &Map<String, Value>,
    ) -> Result<Self, String> {
        let command_array = match config.get("command") {
            Some(value) => value.as_array().ok_or(format!(
                "Invalid 'command' key type for game '{}' (must be an array)",
                game_name
            ))?,
            None => return Err("Missing 'command' key".to_string()),
        };

        if command_array.is_empty() {
            return Err(format!("'command' array is empty for game '{}'", game_name));
        }

        let mut command_array_strings: Vec<String> = vec![];
        for arg in command_array {
            match arg.as_str() {
                Some(s) => command_array_strings.push(s.to_owned()),
                None => {
                    return Err(format!(
                        "Error converting to string in 'command' array for game '{}'",
                        game_name
                    ))
                }
            }
        }

        let mut wait_for_exit = true;
        if let Some(value) = config.get("wait_for_exit") {
            wait_for_exit = match value.as_bool() {
                Some(value) => value,
                None => {
                    return Err(format!(
                        "'wait_for_exit' is not a boolean for game '{}'",
                        game_name
                    ))
                }
            };
        }

        let mut delay_after: Option<u64> = None;
        if let Some(value) = config.get("delay_after") {
            delay_after = match value.as_integer() {
                Some(delay) => Some(delay as u64),
                None => {
                    return Err(format!(
                        "Invalid 'delay_after' value type for game '{}' (must be integer)",
                        game_name
                    ));
                }
            };
        }

        let mut env: Vec<(String, String)> = vec![];
        if let Some(value) = config.get("environment") {
            let environment_table = value.as_table().ok_or(format!(
                "'environment' must be a table in game '{}'",
                game_name
            ))?;

            for environment in environment_table {
                if !environment.1.is_str() {
                    return Err(format!(
                        "Invalid value in 'environment' table '{}' for key '{}' (must be string)",
                        game_name, environment.0
                    ));
                }

                env.push((
                    environment.0.clone(),
                    environment.1.as_str().unwrap().to_string(),
                ));
            }
        }

        return Ok(ExternalCommand {
            id,
            args: command_array_strings,
            env,
            wait_for_exit,
            delay_after,
        });
    }

    pub fn run(&self) -> Result<Option<Child>, std::io::Error> {
        let mut binding = Command::new(&self.args[0]);
        let command = binding.args(&self.args[1..]);

        for environment in &self.env {
            command.env(&environment.0, &environment.1);
        }

        let process_result = command.spawn();

        let process = match process_result {
            Ok(process) => Some(process),
            Err(error) => {
                return Err(error);
            }
        };

        if !self.wait_for_exit {
            if self.delay_after.is_some() {
                thread::sleep(Duration::from_secs(self.delay_after.unwrap()));
            }

            return Ok(process);
        }

        match process.unwrap().wait() {
            Ok(status) => {
                if !status.success() {
                    println!(
                        "Command '{}' ended with non zero status for game '{}'",
                        self.args[0], self.id
                    );
                }
            }
            Err(error) => println!("Process failed to wait for game '{}': {}", self.id, error),
        }

        if self.delay_after.is_some() {
            thread::sleep(Duration::from_secs(self.delay_after.unwrap()));
        }

        return Ok(None);
    }

    /// Moves all elements from variables to this.
    pub fn add_environment_variables(&mut self, variables: &mut Vec<(String, String)>) -> () {
        self.env.append(variables);
    }
}
