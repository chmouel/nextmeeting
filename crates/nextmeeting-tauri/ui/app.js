const timelineNode = document.querySelector("#timeline");
const meetingListNode = document.querySelector("#meetingList");
const actionListNode = document.querySelector("#actionList");
const sourceBadgeNode = document.querySelector("#sourceBadge");
const todayTitleNode = document.querySelector("#todayTitle");
const heroTitleNode = document.querySelector("#heroTitle");
const heroMetaNode = document.querySelector("#heroMeta");
const statusLineNode = document.querySelector("#statusLine");
const joinNowButtonNode = document.querySelector("#joinNowButton");
const createMeetingButtonNode = document.querySelector("#createMeetingButton");

function parseClockOnDate(baseDate, hhmm) {
  const [hourText, minuteText] = String(hhmm || "00:00").split(":");
  const date = new Date(baseDate);
  date.setHours(Number(hourText) || 0, Number(minuteText) || 0, 0, 0);
  return date;
}

function buildTimelineMeetings(meetings, rangeStart, rangeEnd) {
  const startMs = rangeStart.getTime();
  const endMs = rangeEnd.getTime();
  const windowMs = endMs - startMs;

  return meetings
    .map((meeting) => {
      const meetingStart = parseClockOnDate(rangeStart, meeting.startTime);
      let meetingEnd = parseClockOnDate(rangeStart, meeting.endTime);
      if (meetingEnd <= meetingStart) {
        meetingEnd.setDate(meetingEnd.getDate() + 1);
      }

      const clippedStart = Math.max(meetingStart.getTime(), startMs);
      const clippedEnd = Math.min(meetingEnd.getTime(), endMs);

      if (clippedEnd <= clippedStart) {
        return null;
      }

      const leftPercent = ((clippedStart - startMs) / windowMs) * 100;
      const widthPercent = ((clippedEnd - clippedStart) / windowMs) * 100;

      return {
        title: meeting.title,
        leftPercent,
        widthPercent,
        status: meeting.status,
      };
    })
    .filter(Boolean);
}

function renderTimeline(meetings = []) {
  const now = new Date();
  const rangeStart = new Date(now);
  rangeStart.setMinutes(0, 0, 0);
  rangeStart.setHours(rangeStart.getHours() - 2);

  const rangeEnd = new Date(rangeStart);
  rangeEnd.setHours(rangeEnd.getHours() + 8);

  const hours = Array.from({ length: 8 }, (_, idx) => {
    const tickDate = new Date(rangeStart);
    tickDate.setHours(rangeStart.getHours() + idx);
    return String(tickDate.getHours()).padStart(2, "0");
  });

  const spans = buildTimelineMeetings(meetings, rangeStart, rangeEnd);

  const ticksHtml = hours
    .map((hour, idx) => {
      const active = idx === 2 ? "active" : "";
      return `
        <div class="tick ${active}">
          <span>${hour}</span>
          <span class="tick-line"></span>
        </div>
      `;
    })
    .join("");

  const spansHtml = spans
    .map(
      (span) => `
        <div
          class="timeline-meeting ${span.status}"
          style="left: ${span.leftPercent}%; width: ${span.widthPercent}%;"
          title="${span.title}"
        ></div>
      `,
    )
    .join("");

  timelineNode.innerHTML = `
    <div class="timeline-ticks">${ticksHtml}</div>
    <div class="timeline-meetings">${spansHtml}</div>
  `;
}

function renderMeetings(meetings) {
  if (!meetings.length) {
    meetingListNode.innerHTML = '<p class="empty">No meetings scheduled in this window.</p>';
    return;
  }

  meetingListNode.innerHTML = meetings
    .slice(0, 4)
    .map(
      (meeting) => `
        <article class="meeting">
          <p class="meeting-day">${meeting.dayLabel}</p>
          <h3 class="meeting-title">${meeting.title}</h3>
          <p class="meeting-time">${meeting.startTime} - ${meeting.endTime}</p>
          <div class="meeting-service">
            ${meeting.service}
            <span class="meeting-status ${meeting.status}">${meeting.status}</span>
          </div>
        </article>
      `,
    )
    .join("");
}

function renderUtilityActions() {
  const utilityActions = [
    { label: "Open calendar day", command: "open_calendar_day", status: "Opening your calendar..." },
    { label: "Preferences", command: "open_preferences", status: "Opening preferences..." },
    { label: "Quit", command: "quit" },
  ];

  actionListNode.innerHTML = utilityActions
    .map((action) => `<button class="utility-action" type="button">${action.label}</button>`)
    .join("");

  actionListNode.querySelectorAll(".utility-action").forEach((button, index) => {
    button.addEventListener("click", async () => {
      const action = utilityActions[index];
      if (action.command === "quit") {
        await quitApp();
        return;
      }
      await runCommand(action.command, action.status);
    });
  });
}

function renderHero(meetings) {
  const ongoing = meetings.find((meeting) => meeting.status === "ongoing");
  const nextMeeting = meetings[0];

  if (ongoing) {
    heroTitleNode.textContent = `Live now: ${ongoing.title}`;
    heroMetaNode.textContent = `${ongoing.startTime} - ${ongoing.endTime} on ${ongoing.service}`;
    joinNowButtonNode.textContent = "Join live meeting";
    return;
  }

  joinNowButtonNode.textContent = "Join next meeting";
  if (nextMeeting) {
    heroTitleNode.textContent = `Next: ${nextMeeting.title}`;
    heroMetaNode.textContent = `${nextMeeting.startTime} - ${nextMeeting.endTime} on ${nextMeeting.service}`;
    return;
  }

  heroTitleNode.textContent = "No meeting right now";
  heroMetaNode.textContent = "You are free for the moment.";
}

function updateTitleFromMeetings(meetings) {
  if (!meetings.length) {
    todayTitleNode.textContent = "Today";
    return;
  }

  todayTitleNode.textContent = `${meetings[0].dayLabel} agenda`;
}

function fallbackData() {
  return {
    source: "unavailable",
    meetings: [],
    actions: [],
  };
}

function setStatus(message) {
  statusLineNode.textContent = message;
}

async function runCommand(command, successMessage) {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    setStatus("Action is only available in the desktop app.");
    return;
  }

  try {
    await invoke(command);
    setStatus(successMessage);
  } catch (err) {
    setStatus(`Action failed: ${String(err)}`);
  }
}

async function quitApp() {
  const appWindow = window.__TAURI__?.window?.getCurrentWindow;
  if (!appWindow) {
    setStatus("Quit is only available in the desktop app.");
    return;
  }
  await appWindow().close();
}

async function loadDashboard() {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    return fallbackData();
  }

  try {
    return await invoke("get_dashboard_data");
  } catch {
    return fallbackData();
  }
}

function bindPrimaryActions() {
  joinNowButtonNode.addEventListener("click", async () => {
    await runCommand("join_next_meeting", "Opening your next meeting...");
  });

  createMeetingButtonNode.addEventListener("click", async () => {
    await runCommand("create_meeting", "Creating a new meeting...");
  });
}

async function main() {
  bindPrimaryActions();
  renderUtilityActions();

  const dashboard = await loadDashboard();
  sourceBadgeNode.textContent = dashboard.source;
  renderHero(dashboard.meetings || []);
  renderTimeline(dashboard.meetings || []);
  renderMeetings(dashboard.meetings || []);
  updateTitleFromMeetings(dashboard.meetings || []);
  setStatus("");
}

main();
