#! /usr/bin/env node

import { list } from "./commands/list.js";

import { program } from "commander";
import { login } from "./commands/login.js";

program
  .command("list")
  .description("List tracked time registries per project.")
  .action(list);

program.command("login").description("Login to Toggl.").action(login);

program.parse();
