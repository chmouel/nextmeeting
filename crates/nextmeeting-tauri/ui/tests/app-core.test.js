import { describe, expect, it } from "vitest";

import {
  buildMeetingDetailItems,
  buildTimelineMeetings,
  createDashboardApp,
  joinButtonLabel,
  meetingEndDate,
  meetingStartDate,
  parseClockOnDate,
  parseMeetingDate,
  truncateLabel,
} from "../app-core.js";

function createDomFixture() {
  document.body.innerHTML = `
    <main class="panel">
      <div id="timeline"></div>
      <div id="meetingList"></div>
      <div id="actionList"></div>
      <span id="sourceBadge"></span>
      <p id="todayTitle"></p>
      <h1 id="heroTitle"></h1>
      <p id="heroMeta"></p>
      <p id="statusLine"></p>
      <button id="joinNowButton" type="button"></button>
      <button id="createMeetingButton" type="button"></button>
      <details id="heroMeetingDetails" hidden>
        <summary id="heroMeetingDetailsSummary"></summary>
        <div id="heroMeetingDetailsContent"></div>
      </details>
    </main>
  `;
}

function tick() {
  return new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
}

describe("date helpers", () => {
  it("parseClockOnDate applies HH:MM to the base date", () => {
    const base = new Date(2026, 1, 8, 0, 0, 0, 0);
    const parsed = parseClockOnDate(base, "13:45");

    expect(parsed.getHours()).toBe(13);
    expect(parsed.getMinutes()).toBe(45);
  });

  it("parseMeetingDate falls back when value is invalid", () => {
    const base = new Date(2026, 1, 8, 0, 0, 0, 0);
    const parsed = parseMeetingDate("not-a-date", base, "09:30");

    expect(parsed.getHours()).toBe(9);
    expect(parsed.getMinutes()).toBe(30);
  });

  it("meetingStartDate reads startAt when available", () => {
    const meeting = { startAt: "2026-02-08T10:00:00Z", startTime: "07:00" };

    expect(meetingStartDate(meeting).toISOString()).toBe("2026-02-08T10:00:00.000Z");
  });

  it("meetingEndDate rolls over to next day when end <= start", () => {
    const meeting = { startTime: "23:30", endTime: "00:15" };
    const base = new Date(2026, 1, 8, 0, 0, 0, 0);
    const start = meetingStartDate(meeting, base);
    const end = meetingEndDate(meeting, base);

    expect(end.getTime()).toBeGreaterThan(start.getTime());
    expect((end.getTime() - start.getTime()) / 60_000).toBe(45);
  });
});

describe("label and details helpers", () => {
  it("truncateLabel appends ellipsis for long strings", () => {
    expect(truncateLabel("abcdefghij", 6)).toBe("abcde\u2026");
  });

  it("joinButtonLabel uses fallback when meeting title is empty", () => {
    expect(joinButtonLabel({ title: "" }, "Join next")).toBe("Join next meeting");
  });

  it("buildMeetingDetailItems includes expected fields and skips unknown response", () => {
    const items = buildMeetingDetailItems({
      location: "Room 1",
      durationMinutes: 65,
      attendeeCount: 2,
      responseStatus: "unknown",
      calendarId: "work@example.com",
      description: "Planning",
    });

    expect(items).toEqual([
      { label: "Location", value: "Room 1" },
      { label: "Duration", value: "1h 5m" },
      { label: "Attendees", value: "2 attendees" },
      { label: "Calendar", value: "work@example.com" },
      { label: "Description", value: "Planning" },
    ]);
  });
});

describe("timeline helper", () => {
  it("clips bars to visible range and drops non-overlapping meetings", () => {
    const rangeStart = new Date("2026-02-08T10:00:00Z");
    const rangeEnd = new Date("2026-02-08T18:00:00Z");
    const meetings = [
      {
        title: "Before window",
        startAt: "2026-02-08T08:00:00Z",
        endAt: "2026-02-08T11:00:00Z",
        status: "soon",
      },
      {
        title: "After window",
        startAt: "2026-02-08T19:00:00Z",
        endAt: "2026-02-08T20:00:00Z",
        status: "upcoming",
      },
    ];

    const spans = buildTimelineMeetings(meetings, rangeStart, rangeEnd);

    expect(spans).toHaveLength(1);
    expect(spans[0].title).toBe("Before window");
    expect(spans[0].leftPercent).toBe(0);
    expect(spans[0].widthPercent).toBe(12.5);
  });
});

describe("dashboard app integration", () => {
  it("applyDashboard shows no-meeting copy only when source is available", () => {
    createDomFixture();
    const app = createDashboardApp({ document, window, now: () => new Date("2026-02-08T12:00:00Z") });

    app.applyDashboard({ source: "live", meetings: [] });
    expect(document.querySelector("#heroTitle").textContent).toBe("No meeting right now");

    app.applyDashboard({ source: "unavailable", meetings: [] });
    expect(document.querySelector("#heroTitle").textContent).toBe("");
  });

  it("action command path reports desktop-only fallback without tauri bridge", async () => {
    createDomFixture();
    const app = createDashboardApp({
      document,
      window,
      now: () => new Date("2026-02-08T12:00:00Z"),
      setIntervalFn: () => 0,
    });

    await app.main();
    const refreshButton = [...document.querySelectorAll("#actionList .utility-action")].find(
      (button) => button.textContent === "Refresh calendars",
    );

    refreshButton.click();
    await tick();

    expect(document.querySelector("#statusLine").textContent).toBe(
      "Action is only available in the desktop app.",
    );
  });
});
