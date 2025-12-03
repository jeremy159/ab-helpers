import cron from "node-cron";
import { CronExpressionParser } from "cron-parser";
import { applyKiaInterest } from "./apply-kia-interest.js";
import { applyMortgageInterest } from "./apply-mortgage-interest.js";

const timezone = "America/New_York";

/**
 * Get next run time for a cron expression in the specified timezone
 */
const getNextRun = (cronExpression, tz = timezone) => {
  const interval = CronExpressionParser.parse(cronExpression, {
    tz,
    currentDate: new Date(),
  });

  const next = interval.next();

  return {
    iso: next.toISOString(),
    local: next.toString(),
    formatted: next.toDate().toLocaleString("en-US", {
      timeZone: tz,
      dateStyle: "full",
      timeStyle: "long",
    }),
  };
};

// Every Thursday at 8 AM
const weeklyCronExpression = "0 8 * * 4";

const weeklyNext = getNextRun(weeklyCronExpression);
console.info("Next weekly run:");
console.info("  ", weeklyNext.formatted);

cron.schedule(
  weeklyCronExpression,
  async () => {
    console.info("Running Kia interest calculation");
    await applyKiaInterest();

    const next = getNextRun(weeklyCronExpression);
    console.info("Next run:");
    console.info("  ", next.formatted);
  },
  {
    timezone,
  }
);

// Every 18th of the month at 8 AM
const monthlyCronExpression = "0 8 18 * *";

const monthlyNext = getNextRun(monthlyCronExpression);
console.info("Next monthly run:");
console.info("  ", monthlyNext.formatted);

cron.schedule(
  monthlyCronExpression,
  async () => {
    console.info("Running Mortgage interest calculation");
    await applyMortgageInterest();

    const next = getNextRun(monthlyCronExpression);
    console.info("Next run:");
    console.info("  ", next.formatted);
  },
  {
    timezone,
  }
);
