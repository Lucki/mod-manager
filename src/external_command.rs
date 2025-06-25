use std::{
    collections::HashMap,
    process::{Child, Command},
    thread,
    time::Duration,
};

use crate::config::{CommandConfig, EnvironmentConfig};

#[derive(Clone)]
pub struct ExternalCommand {
    config: CommandConfig,
}

impl ExternalCommand {
    pub fn from_config(config: &CommandConfig) -> Result<Self, String> {
        let command_array = &config.command;
        if command_array.is_empty() {
            return Err(format!("'command' array is empty"));
        }

        return Ok(ExternalCommand {
            config: config.clone(),
        });
    }

    pub fn run(&self) -> Result<Option<Child>, std::io::Error> {
        let mut binding = Command::new(&self.config.command[0]);
        let command = binding.args(&self.config.command[1..]);

        match &self.config.environment {
            Some(env) => {
                for (k, v) in &env.variables {
                    command.env(&k, &v);
                }
            }
            None => {}
        }

        let process_result = command.spawn();

        let process = match process_result {
            Ok(process) => Some(process),
            Err(error) => {
                return Err(error);
            }
        };

        if !self.get_wait_for_exit() {
            if self.config.delay_after.is_some() {
                thread::sleep(Duration::from_secs(self.get_delay()));
            }

            return Ok(process);
        }

        match process.unwrap().wait() {
            Ok(status) => {
                if !status.success() {
                    println!(
                        "Command '{}' ended with non zero status",
                        self.config.command[0]
                    );
                }
            }
            Err(error) => println!("Process failed to wait: {}", error),
        }

        if self.config.delay_after.is_some() {
            thread::sleep(Duration::from_secs(self.get_delay()));
        }

        Ok(None)
    }

    fn get_delay(&self) -> u64 {
        match self.config.delay_after {
            Some(delay) => delay,
            None => 0,
        }
    }

    fn get_wait_for_exit(&self) -> bool {
        if self.config.wait_for_exit.is_some_and(|b| b == true) {
            return true;
        }

        false
    }

    pub fn get_id(&self) -> &str {
        &self.config.command[0]
    }

    /// Copy all elements from variables to this.
    pub fn add_environment_variables(&mut self, variables: &HashMap<String, String>) -> () {
        if !self.config.environment.is_some() {
            self.config.environment = Some(EnvironmentConfig {
                variables: HashMap::new(),
            });
        }

        for (k, v) in variables {
            self.config
                .environment
                .as_mut()
                .unwrap()
                .variables
                .insert(k.clone(), v.clone());
        }
    }
}
