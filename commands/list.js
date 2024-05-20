import chalk from "chalk";
import fetch from "node-fetch";
import { Buffer } from "buffer";
import dayjs from "dayjs";
import utc from "dayjs/plugin/utc.js";
import timezone from "dayjs/plugin/timezone.js";
import isBetween from "dayjs/plugin/isBetween.js";
import fs from "fs";
import path from "path";
import os from "os";
import createPrompt from "prompt-sync";

const prompt = createPrompt({});

dayjs.extend(utc);
dayjs.extend(timezone);
dayjs.extend(isBetween);

const filePath = path.join(os.homedir(), ".toggl2tsc");

const token = fs.readFileSync(filePath, "utf8");

const base64Credentials = Buffer.from(`${token}:api_token`).toString("base64");

const currTz = "Europe/Brussels";
const yesterday = dayjs()
  .subtract(1, "days")
  .tz(currTz)
  .startOf("day")
  .toISOString();
const today = dayjs().tz(currTz).startOf("day").toISOString();
const tomorrow = dayjs().add(1, "days").tz(currTz).startOf("day").toISOString();

const timeEntriesUrl = `https://api.track.toggl.com/api/v9/me/time_entries?start_date=${today}&end_date=${tomorrow}`;

const timeEntriesJson = await fetchTimeEntries();

async function fetchTimeEntries() {
  const timeEntriesResponse = await fetch(timeEntriesUrl, {
    method: "GET",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Basic ${base64Credentials}`,
    },
  });

  if (!timeEntriesResponse.ok) {
    console.error("Failed to fetch data from Toggl API");
    return [];
  }

  return await timeEntriesResponse.json();
}

async function fetchWorkspaces() {
  const workspacesUrl = `https://api.track.toggl.com/api/v9/workspaces`;

  const workspacesResponse = await fetch(workspacesUrl, {
    method: "GET",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Basic ${base64Credentials}`,
    },
  });

  if (!workspacesResponse.ok) {
    console.error("Failed to fetch workspaces from Toggl API");
    return [];
  }

  return await workspacesResponse.json();
}

async function selectWorkspaceId() {
  const workspaces = await fetchWorkspaces();

  if (workspaces.length === 0) {
    console.error("No workspaces found");
    return null;
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

export async function list() {
  const workspaceId = await selectWorkspaceId();
  if (!workspaceId) {
    return;
  }

  const projectsUrl = `https://api.track.toggl.com/api/v9/workspaces/${workspaceId}/projects`;

  const projectsResponse = await fetch(projectsUrl, {
    method: "GET",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Basic ${base64Credentials}`,
    },
  });

  if (!projectsResponse.ok) {
    console.error("Failed to fetch data from Toggl API");
    return;
  }

  const projectsJson = await projectsResponse.json();

  const groupedEntries = timeEntriesJson.reduce((acc, entry) => {
    const projectId = entry.project_id;
    if (!acc[projectId]) {
      acc[projectId] = {
        name: null,
        entries: [],
        totalHours: 0,
      };
    }
    acc[projectId].entries.push(entry);
    acc[projectId].totalHours += entry.duration / 3600;
    return acc;
  }, {});

  const projectNamesMap = {};
  projectsJson.forEach((project) => {
    projectNamesMap[project.id] = project.name;
  });

  console.log(chalk.green("Your current time entries:"));
  console.log();

  Object.keys(groupedEntries).forEach((projectId) => {
    const projectName = projectNamesMap[projectId];
    const projectData = groupedEntries[projectId];

    console.log(chalk.green(`${projectName}`));
    console.log(chalk.green("+".repeat(projectName.length)));

    console.log(
      chalk.white(`Total hours today: ${projectData.totalHours.toFixed(2)}`),
    );
    console.log();

    console.log(chalk.cyan("Tickets:"));
    projectData.entries.forEach((entry) => {
      const durationHours = entry.duration / 3600;
      console.log(`• ${entry.description} (${durationHours.toFixed(2)})`);
    });

    console.log();
    console.log(chalk.red("####################"));
    console.log();
  });

  const yesterdayEntries = timeEntriesJson.filter((entry) =>
    dayjs(entry.start).isBetween(yesterday, today),
  );
  const totalHoursYesterday = yesterdayEntries.reduce(
    (acc, entry) => acc + entry.duration / 3600,
    0,
  );

  console.log(chalk.yellow("============================="));
  const totalHoursToday = Object.values(groupedEntries).reduce(
    (acc, project) => acc + project.totalHours,
    0,
  );
  if (totalHoursToday < 8) {
    console.log(chalk.red(`Total hours today: ${totalHoursToday.toFixed(2)}`));
  } else {
    console.log(
      chalk.green(`Total hours today: ${totalHoursToday.toFixed(2)}`),
    );
  }
  console.log(
    chalk.yellow(`Total hours yesterday: ${totalHoursYesterday.toFixed(2)}`),
  );

  console.log();
}
