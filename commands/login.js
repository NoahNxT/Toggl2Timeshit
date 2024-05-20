import chalk from "chalk";
import createPrompt from "prompt-sync";
import fs from "fs";
import os from "os";
import path from "path";

const prompt = createPrompt({});

export async function login() {
  console.log(chalk.green("Login to Toggl"));
  console.log();
  console.log(chalk.yellow("Please enter your Toggl API token:"));
  console.log();
  console.log(
    chalk.gray("You can find your API token in your Toggl profile settings"),
  );
  console.log();
  const token = prompt(chalk.yellow("API token: "));
  console.log();

  const filePath = path.join(os.homedir(), ".toggl2tsc");
  fs.writeFileSync(filePath, token);

  console.log(chalk.green("API token saved successfully!"));
  console.log();
  console.log(chalk.green("You are now logged in!"));
}
