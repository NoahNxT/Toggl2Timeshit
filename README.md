![GitHub Release Date](https://img.shields.io/github/release-date/NoahNxT/Toggl2Timeshit)
![GitHub code size in bytes](https://img.shields.io/github/languages/code-size/NoahNxT/Toggl2Timeshit)
![NPM Version](https://img.shields.io/npm/v/toggl2timeshit)
![X (formerly Twitter) Follow](https://img.shields.io/twitter/follow/does_it_code)

# Toggl2Timeshit

Toggl2Timeshit is a simple yet powerful tool to convert Toggl Track reports into a user-friendly timesheet format.

## Features
- Fetch time entries from Toggl Track.
- Group and display time entries by project.
- Summarize total hours spent per project and per day.
- Support for custom date ranges.

## Installation

Install the package globally using npm:
```bash
npm install -g toggl2timeshit
```

## Authentication
 
You can find it in your [Toggl Track Profile](https://track.toggl.com/profile).`

```bash
npx timeshit login
```

## Usage
### Generate Timesheet for Today

Run the command to generate the timesheet for today:
```bash
npx timeshit list
```

### Generate Timesheet for a Custom Date Range
You can specify a custom date range using the --start-date (-sd) and --end-date (-ed) options:
```bash
npx timeshit list --start-date YYYY-MM-DD --end-date YYYY-MM-DD
```

### Generate Timesheet for a Specific Date
You can specify a specific date using the --date (-d) option alone:
```bash
npx timeshit list --date YYYY-MM-DD
```


## Generate Timesheets from a Date until Today
You can specify a custom date range using the --start-date (-sd) option alone:

```bash
npx timeshit list --start-date YYYY-MM-DD
```

## Example Output
```bash
Your current time entries:

Project A
+++++++++
Total hours: 0.60

Tickets:
• Entry 1 name (0.27)
• Entry 2 name (0.33)

####################

Project B
+++++++++
Total hours: 1.56

Tickets:
• Entry 1 name (0.61)
• Entry 2 name (0.61)
• Entry 3 name (0.34)

####################

Project C
+++++++++
Total hours: 0.53

Tickets:
• Entry 1 name (0.53)

####################

=============================
Total hours today: 3.49
Total hours yesterday: 0.00
```

## Contributing
We welcome contributions to Toggl2Timeshit! If you have any improvements or bug fixes, please open an issue or submit a pull request on GitHub.

## License
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Connect
- Follow me on Twitter: [@does_it_code](https://twitter.com/does_it_code)
- Connect with me on LinkedIn: [Noah Gillard](https://www.linkedin.com/in/noah-gillard/)

Feel free to reach out with any questions or feedback! Enjoy using Toggl2Timeshit to simplify your timesheet generation.

