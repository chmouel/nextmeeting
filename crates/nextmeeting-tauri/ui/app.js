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
const panelNode = document.querySelector(".panel");

const REFRESH_INTERVAL_MS = 60_000;

function escapeHtml(str) {
  const div = document.createElement("div");
  div.textContent = str;
  return div.innerHTML;
}

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
  const currentHour = now.getHours();
  const rangeStart = new Date(now);
  rangeStart.setMinutes(0, 0, 0);
  rangeStart.setHours(rangeStart.getHours() - 2);

  const rangeEnd = new Date(rangeStart);
  rangeEnd.setHours(rangeEnd.getHours() + 8);

  const hours = Array.from({ length: 8 }, (_, idx) => {
    const tickDate = new Date(rangeStart);
    tickDate.setHours(rangeStart.getHours() + idx);
    return { label: String(tickDate.getHours()).padStart(2, "0"), hour: tickDate.getHours() };
  });

  const spans = buildTimelineMeetings(meetings, rangeStart, rangeEnd);

  const ticksContainer = document.createElement("div");
  ticksContainer.className = "timeline-ticks";
  for (const { label, hour } of hours) {
    const tick = document.createElement("div");
    tick.className = hour === currentHour ? "tick active" : "tick";
    const labelSpan = document.createElement("span");
    labelSpan.textContent = label;
    const lineSpan = document.createElement("span");
    lineSpan.className = "tick-line";
    tick.appendChild(labelSpan);
    tick.appendChild(lineSpan);
    ticksContainer.appendChild(tick);
  }

  const meetingsContainer = document.createElement("div");
  meetingsContainer.className = "timeline-meetings";
  for (const span of spans) {
    const bar = document.createElement("div");
    bar.className = `timeline-meeting ${span.status}`;
    bar.style.left = `${span.leftPercent}%`;
    bar.style.width = `${span.widthPercent}%`;
    bar.title = span.title;
    meetingsContainer.appendChild(bar);
  }

  timelineNode.innerHTML = "";
  timelineNode.appendChild(ticksContainer);
  timelineNode.appendChild(meetingsContainer);
}

function renderMeetings(meetings) {
  meetingListNode.innerHTML = "";

  if (!meetings.length) {
    return;
  }

  for (const meeting of meetings.slice(0, 4)) {
    const article = document.createElement("article");
    article.className = meeting.joinUrl ? "meeting meeting-clickable" : "meeting";

    const dayP = document.createElement("p");
    dayP.className = "meeting-day";
    dayP.textContent = meeting.dayLabel;

    const titleH3 = document.createElement("h3");
    titleH3.className = "meeting-title";
    titleH3.textContent = meeting.title;

    const timeP = document.createElement("p");
    timeP.className = "meeting-time";
    timeP.textContent = `${meeting.startTime} - ${meeting.endTime}`;

    if (meeting.relativeTime) {
      const relSpan = document.createElement("span");
      relSpan.className = "meeting-relative";
      relSpan.textContent = ` \u00b7 ${meeting.relativeTime}`;
      timeP.appendChild(relSpan);
    }

    const serviceDiv = document.createElement("div");
    serviceDiv.className = "meeting-service";
    serviceDiv.textContent = meeting.service;

    const statusSpan = document.createElement("span");
    statusSpan.className = `meeting-status ${meeting.status}`;
    statusSpan.textContent = meeting.status;
    serviceDiv.appendChild(statusSpan);

    article.appendChild(dayP);
    article.appendChild(titleH3);
    article.appendChild(timeP);
    article.appendChild(serviceDiv);

    if (meeting.joinUrl) {
      article.addEventListener("click", async () => {
        const invoke = window.__TAURI__?.core?.invoke;
        if (invoke) {
          try {
            await invoke("join_meeting_by_url", { url: meeting.joinUrl });
            setStatus("Opening meeting...");
          } catch (err) {
            setStatus(`Failed to open meeting: ${String(err)}`);
          }
        }
      });
    }

    meetingListNode.appendChild(article);
  }
}

function renderUtilityActions() {
  const utilityActions = [
    { label: "Refresh calendars", command: "refresh_meetings", status: "Refreshing calendar data..." },
    { label: "Open calendar day", command: "open_calendar_day", status: "Opening your calendar..." },
    { label: "Snooze 15 min", command: "snooze_notifications", args: { minutes: 15 }, status: "Snoozed for 15 minutes" },
    { label: "Snooze 30 min", command: "snooze_notifications", args: { minutes: 30 }, status: "Snoozed for 30 minutes" },
    { label: "Snooze 1 hour", command: "snooze_notifications", args: { minutes: 60 }, status: "Snoozed for 1 hour" },
    { label: "Preferences", command: "open_preferences", status: "Opening preferences..." },
    { label: "Quit", command: "quit" },
  ];

  actionListNode.innerHTML = "";
  for (const action of utilityActions) {
    const button = document.createElement("button");
    button.className = "utility-action";
    button.type = "button";
    button.textContent = action.label;

    button.addEventListener("click", async () => {
      if (action.command === "quit") {
        await quitApp();
        return;
      }
      await runCommand(action.command, action.status, action.args);
    });

    actionListNode.appendChild(button);
  }
}

function renderHero(meetings) {
  const ongoing = meetings.find((meeting) => meeting.status === "ongoing");
  const nextMeeting = meetings[0];

  if (ongoing) {
    heroTitleNode.textContent = `Live now: ${ongoing.title}`;
    const meta = `${ongoing.startTime} - ${ongoing.endTime} on ${ongoing.service}`;
    heroMetaNode.textContent = ongoing.relativeTime ? `${meta} \u00b7 ${ongoing.relativeTime}` : meta;
    joinNowButtonNode.textContent = "Join live meeting";
    joinNowButtonNode.style.display = "";
    return;
  }

  if (nextMeeting) {
    joinNowButtonNode.textContent = "Join next meeting";
    joinNowButtonNode.style.display = "";
    heroTitleNode.textContent = `Next: ${nextMeeting.title}`;
    const meta = `${nextMeeting.startTime} - ${nextMeeting.endTime} on ${nextMeeting.service}`;
    heroMetaNode.textContent = nextMeeting.relativeTime ? `${meta} \u00b7 ${nextMeeting.relativeTime}` : meta;
    return;
  }

  joinNowButtonNode.style.display = "none";
  heroTitleNode.textContent = "";
  heroMetaNode.textContent = "";
}

function updateTitleFromMeetings(meetings) {
  if (!meetings.length) {
    todayTitleNode.textContent = "";
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

function setConnectionHealth(source) {
  const dot = sourceBadgeNode.querySelector(".health-dot") || document.createElement("span");
  dot.className = "health-dot";

  if (source === "unavailable") {
    dot.classList.add("disconnected");
    sourceBadgeNode.title = "Disconnected from server";
  } else {
    dot.classList.add("connected");
    sourceBadgeNode.title = `Connected (${source})`;
  }

  if (!sourceBadgeNode.contains(dot)) {
    sourceBadgeNode.textContent = "";
    sourceBadgeNode.appendChild(dot);
  }
}

function showLoading() {
  panelNode.classList.add("loading");
  heroTitleNode.textContent = "Loading meetings...";
  heroMetaNode.textContent = "Connecting to server";
}

function hideLoading() {
  panelNode.classList.remove("loading");
}

async function runCommand(command, successMessage, args) {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    setStatus("Action is only available in the desktop app.");
    return;
  }

  try {
    await invoke(command, args || {});
    setStatus(successMessage);
    if (command === "refresh_meetings") {
      await refreshDashboard();
    }
  } catch (err) {
    setStatus(`Action failed: ${String(err)}`);
  }
}

async function quitApp() {
  const invoke = window.__TAURI__?.core?.invoke;
  if (invoke) {
    await invoke("quit_app");
    return;
  }
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

function applyDashboard(dashboard) {
  const meetings = dashboard.meetings || [];
  setConnectionHealth(dashboard.source);
  renderHero(meetings);
  renderTimeline(meetings);
  renderMeetings(meetings);
  updateTitleFromMeetings(meetings);

  if (!meetings.length && dashboard.source !== "unavailable") {
    heroTitleNode.textContent = "No meeting right now";
    heroMetaNode.textContent = "You are free for the moment.";
    joinNowButtonNode.style.display = "none";
  }
}

async function refreshDashboard() {
  const dashboard = await loadDashboard();
  applyDashboard(dashboard);
  setStatus("");
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

  showLoading();
  const dashboard = await loadDashboard();
  hideLoading();
  applyDashboard(dashboard);
  setStatus("");

  setInterval(refreshDashboard, REFRESH_INTERVAL_MS);
}

main();
