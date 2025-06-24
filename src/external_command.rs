use std::{
    collections::HashMap,
    process::{Child, Command},
    thread,
    time::Duration,
};

use crate::config::CommandConfig;

#[derive(Clone, Debug)]
pub struct ExternalCommand {
    pub id: String,
    args: Vec<String>,
    env: HashMap<String, String>,
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
            env: HashMap::new(),
            wait_for_exit: wait_for_exit.unwrap_or(false),
            delay_after,
        }
    }

    pub fn from_config(
        game_name: String,
        id: String,
        config: &CommandConfig,
    ) -> Result<Self, String> {
        let command_array = &config.command;
        if command_array.is_empty() {
            return Err(format!("'command' array is empty for game '{}'", game_name));
        }

        let wait_for_exit = match config.wait_for_exit {
            Some(value) => value,
            None => false,
        };

        let env = match &config.environment {
            Some(environment) => environment.variables.clone(),
            None => HashMap::new(),
        };

        return Ok(ExternalCommand {
            id,
            args: command_array.clone(),
            env,
            wait_for_exit,
            delay_after: config.delay_after,
        });
    }

    pub fn run(&self) -> Result<Option<Child>, std::io::Error> {
        let mut binding = Command::new(&self.args[0]);
        let command = binding.args(&self.args[1..]);

        for (k, v) in &self.env {
            command.env(&k, &v);
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

    /// Copy all elements from variables to this.
    pub fn add_environment_variables(&mut self, variables: &HashMap<String, String>) -> () {
        for (k, v) in variables {
            self.env.insert(k.clone(), v.clone());
        }
    }
}
