import chalk from "chalk";
import createPrompt from "prompt-sync";
import { fetchWorkspaces } from "../api/toggl.js";

const prompt = createPrompt({});

export async function selectWorkspaceId() {
  const workspaces = await fetchWorkspaces();

  if (!workspaces || workspaces.length === 0) {
    console.error("No workspaces found");
    return null;
  }

  if (workspaces.length === 1) {
    console.log(
      chalk.green(
        `Automatically selecting the only workspace: ${chalk.blue(workspaces[0].name)}`,
      ),
    );
    return workspaces[0].id;
  }

  console.log(chalk.green("Select a workspace ID:"));
  workspaces.forEach((workspace, index) => {
    console.log(chalk.blueBright(`${index + 1}. ${workspace.name}`));
  });

  const userInput = prompt(
    "Enter the number corresponding to the workspace ID: ",
  );
  const selectedIndex = parseInt(userInput);

  if (
    isNaN(selectedIndex) ||
    selectedIndex < 1 ||
    selectedIndex > workspaces.length
  ) {
    console.error("Invalid selection");
    return null;
  }

  return workspaces[selectedIndex - 1].id;
}
