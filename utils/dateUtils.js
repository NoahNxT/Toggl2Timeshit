import dayjs from "dayjs";
import utc from "dayjs/plugin/utc.js";
import timezone from "dayjs/plugin/timezone.js";
import isBetween from "dayjs/plugin/isBetween.js";

dayjs.extend(utc);
dayjs.extend(timezone);
dayjs.extend(isBetween);

const currTz = "Europe/Brussels";

export function getCurrentDay() {
  return dayjs().tz(currTz).startOf("day").toISOString();
}

export function getNextDay() {
  return dayjs().add(1, "days").tz(currTz).startOf("day").toISOString();
}

export function getStartOfDay(date) {
  return dayjs(date).tz(currTz).startOf("day").toISOString();
}

export function getEndOfDay(date) {
  return dayjs(date).tz(currTz).endOf("day").toISOString();
}

export function getEndOfToday() {
  return dayjs().tz(currTz).endOf("day").toISOString();
}

export function isBetweenDates(entry, start, end) {
  return dayjs(entry.start).isBetween(start, end);
}
