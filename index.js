#! /usr/bin/env node

import { list } from "./commands/list.js";

import { program } from "commander";
import { login } from "./commands/login.js";

program
  .command("list")
  .option("-sd, --start-date <date>", "Start date for time entries YYYY-MM-DD")
  .option("-ed, --end-date <date>", "End date for time entries YYYY-MM-DD")
  .option("-d, --date <date>", "Date for time entries YYYY-MM-DD")
  .description("List tracked time registries per project.")
  .action(list);

program.command("login").description("Login to Toggl.").action(login);

program.parse(process.argv);
