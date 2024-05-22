import fetch from "node-fetch";
import { Buffer } from "buffer";
import fs from "fs";
import path from "path";
import os from "os";

const filePath = path.join(os.homedir(), ".toggl2tsc");
const token = fs.readFileSync(filePath, "utf8");
const base64Credentials = Buffer.from(`${token}:api_token`).toString("base64");

async function fetchFromToggl(url) {
  const response = await fetch(url, {
    method: "GET",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Basic ${base64Credentials}`,
    },
  });

  if (!response.ok) {
    console.error(
      `Failed to fetch data from Toggl API: ${response.statusText}`,
    );
    return null;
  }

  return await response.json();
}

export async function fetchTimeEntries(start, end) {
  const url = `https://api.track.toggl.com/api/v9/me/time_entries?start_date=${start}&end_date=${end}`;
  return await fetchFromToggl(url);
}

export async function fetchWorkspaces() {
  const url = `https://api.track.toggl.com/api/v9/workspaces`;
  return await fetchFromToggl(url);
}

export async function fetchProjects(workspaceId) {
  const url = `https://api.track.toggl.com/api/v9/workspaces/${workspaceId}/projects`;
  return await fetchFromToggl(url);
}
