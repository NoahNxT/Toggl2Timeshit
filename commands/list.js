import chalk from "chalk";
import { fetchTimeEntries, fetchProjects } from "../api/toggl.js";
import {
  getStartOfDay,
  getEndOfDay,
  getCurrentDay,
  getNextDay,
  isBetweenDates,
  getEndOfToday,
} from "../utils/dateUtils.js";
import { selectWorkspaceId } from "../utils/promptUtils.js";

export async function list(options) {
  const { startDate, endDate, date } = options;

  let start, end;
  if (date) {
    start = getStartOfDay(date);
    end = getEndOfDay(date);
  } else if (startDate && endDate) {
    start = getStartOfDay(startDate);
    end = getEndOfDay(endDate);
  } else if (startDate) {
    start = getStartOfDay(startDate);
    end = endDate ? getEndOfDay(endDate) : getEndOfToday();
  } else if (!startDate && endDate) {
    console.error(chalk.red("Please provide a start date"));
    return;
  } else {
    start = getCurrentDay();
    end = getNextDay();
  }

  const workspaceId = await selectWorkspaceId();
  if (!workspaceId) {
    return;
  }

  const timeEntriesJson = await fetchTimeEntries(start, end);
  if (!timeEntriesJson) return;

  const projectsJson = await fetchProjects(workspaceId);
  if (!projectsJson) return;

  const validTimeEntries = timeEntriesJson.filter(
    (entry) => entry.stop !== null,
  );

  const groupedEntries = validTimeEntries.reduce((acc, entry) => {
    const projectId = entry.project_id;
    if (!acc[projectId]) {
      acc[projectId] = { entries: {}, totalHours: 0 };
    }
    if (!acc[projectId].entries[entry.description]) {
      acc[projectId].entries[entry.description] = {
        description: entry.description,
        totalDuration: 0,
      };
    }
    acc[projectId].entries[entry.description].totalDuration += entry.duration;
    acc[projectId].totalHours += entry.duration / 3600;
    return acc;
  }, {});

  const projectNamesMap = projectsJson.reduce((map, project) => {
    map[project.id] = project.name;
    return map;
  }, {});

  console.log(chalk.green("Your current time entries:"));
  console.log();

  Object.keys(groupedEntries).forEach((projectId) => {
    const projectName = projectNamesMap[projectId];
    const projectData = groupedEntries[projectId];

    console.log(chalk.green(`${projectName}`));
    console.log(chalk.green("+".repeat(projectName.length)));
    console.log(
      chalk.white(`Total hours: ${projectData.totalHours.toFixed(2)}`),
    );
    console.log();

    console.log(chalk.cyan("Tickets:"));
    Object.values(projectData.entries).forEach((entry) => {
      const durationHours = entry.totalDuration / 3600;
      console.log(`• ${entry.description} (${durationHours.toFixed(2)})`);
    });

    console.log();
    console.log(chalk.red("####################"));
    console.log();
  });

  const filteredEntries = validTimeEntries.filter((entry) =>
    isBetweenDates(entry, start, end),
  );
  const totalHours = filteredEntries.reduce(
    (acc, entry) => acc + entry.duration / 3600,
    0,
  );

  console.log(chalk.yellow("============================="));
  if (totalHours < 8) {
    console.log(chalk.red(`Total hours: ${totalHours.toFixed(2)}`));
  } else {
    console.log(chalk.green(`Total hours: ${totalHours.toFixed(2)}`));
  }
  console.log();
}
