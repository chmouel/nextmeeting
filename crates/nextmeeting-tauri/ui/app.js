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
const heroMeetingDetailsNode = document.querySelector("#heroMeetingDetails");
const heroMeetingDetailsSummaryNode = document.querySelector("#heroMeetingDetailsSummary");
const heroMeetingDetailsContentNode = document.querySelector("#heroMeetingDetailsContent");
const panelNode = document.querySelector(".panel");

const REFRESH_INTERVAL_MS = 60_000;
const isMac = navigator.platform.toUpperCase().includes("MAC");
const timeFormatter = new Intl.DateTimeFormat(undefined, {
  hour: "numeric",
  minute: "2-digit",
});
const dayFormatter = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  day: "numeric",
  month: "short",
});

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

function parseMeetingDate(value, fallbackBaseDate, fallbackTime) {
  if (value) {
    const parsed = new Date(value);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed;
    }
  }
  return parseClockOnDate(fallbackBaseDate, fallbackTime);
}

function meetingStartDate(meeting, baseDate = new Date()) {
  return parseMeetingDate(meeting.startAt, baseDate, meeting.startTime);
}

function meetingEndDate(meeting, baseDate = new Date()) {
  const start = meetingStartDate(meeting, baseDate);
  const end = parseMeetingDate(meeting.endAt, baseDate, meeting.endTime);
  if (end <= start) {
    end.setDate(end.getDate() + 1);
  }
  return end;
}

function formatMeetingRange(meeting) {
  const start = meetingStartDate(meeting);
  const end = meetingEndDate(meeting, start);
  return `${timeFormatter.format(start)} - ${timeFormatter.format(end)}`;
}

function formatMeetingDay(meeting) {
  const start = meetingStartDate(meeting);
  return dayFormatter.format(start);
}

function truncateLabel(value, maxLength = 34) {
  const text = String(value || "").trim();
  if (text.length <= maxLength) {
    return text;
  }
  return `${text.slice(0, maxLength - 1)}\u2026`;
}

function joinButtonLabel(meeting, prefix = "Join") {
  const title = truncateLabel(meeting?.title, 34);
  if (!title) {
    return `${prefix} meeting`;
  }
  return `${prefix}: ${title}`;
}

function buildMeetingDetailItems(meeting) {
  const items = [];

  if (meeting.location) {
    items.push({ label: "Location", value: meeting.location });
  }

  if (meeting.durationMinutes) {
    const hrs = Math.floor(meeting.durationMinutes / 60);
    const mins = meeting.durationMinutes % 60;
    const parts = [];
    if (hrs > 0) parts.push(`${hrs}h`);
    if (mins > 0) parts.push(`${mins}m`);
    items.push({ label: "Duration", value: parts.join(" ") || "0m" });
  }

  if (meeting.attendeeCount > 0) {
    const label = meeting.attendeeCount === 1 ? "1 attendee" : `${meeting.attendeeCount} attendees`;
    items.push({ label: "Attendees", value: label });
  }

  if (meeting.responseStatus && meeting.responseStatus !== "unknown") {
    items.push({
      label: "Your status",
      value: meeting.responseStatus,
      className: `response-${meeting.responseStatus}`,
    });
  }

  if (meeting.calendarId) {
    items.push({ label: "Calendar", value: meeting.calendarId });
  }

  if (meeting.description) {
    items.push({ label: "Description", value: meeting.description });
  }

  return items;
}

function appendMeetingDetailRows(container, items) {
  for (const item of items) {
    const row = document.createElement("div");
    row.className = "detail-item";

    const labelSpan = document.createElement("span");
    labelSpan.className = "detail-label";
    labelSpan.textContent = item.label;

    const valueSpan = document.createElement("span");
    valueSpan.className = item.className ? `detail-value ${item.className}` : "detail-value";
    valueSpan.textContent = item.value;

    row.appendChild(labelSpan);
    row.appendChild(valueSpan);
    container.appendChild(row);
  }
}

function buildTimelineMeetings(meetings, rangeStart, rangeEnd) {
  const startMs = rangeStart.getTime();
  const endMs = rangeEnd.getTime();
  const windowMs = endMs - startMs;

  return meetings
    .map((meeting) => {
      const meetingStart = meetingStartDate(meeting, rangeStart);
      const meetingEnd = meetingEndDate(meeting, meetingStart);

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

function createMeetingDetails(meeting) {
  const details = document.createElement("details");
  details.className = "meeting-details";

  const summary = document.createElement("summary");
  summary.className = "meeting-details-toggle";
  summary.textContent = "Details";
  details.appendChild(summary);

  const content = document.createElement("div");
  content.className = "meeting-details-content";
  const items = buildMeetingDetailItems(meeting);
  appendMeetingDetailRows(content, items);

  const dismissBtn = document.createElement("button");
  dismissBtn.className = "btn-dismiss";
  dismissBtn.type = "button";
  dismissBtn.textContent = "Dismiss event";
  dismissBtn.addEventListener("click", async (e) => {
    e.stopPropagation();
    await dismissEvent(meeting.id);
  });
  content.appendChild(dismissBtn);

  details.appendChild(content);
  return details;
}

function clearHeroDetails() {
  heroMeetingDetailsNode.hidden = true;
  heroMeetingDetailsNode.open = false;
  heroMeetingDetailsSummaryNode.removeAttribute("aria-label");
  heroMeetingDetailsContentNode.textContent = "";
}

function renderHeroDetails(meeting) {
  if (!meeting) {
    clearHeroDetails();
    return;
  }

  const items = buildMeetingDetailItems(meeting);
  if (!items.length) {
    clearHeroDetails();
    return;
  }

  heroMeetingDetailsContentNode.textContent = "";
  appendMeetingDetailRows(heroMeetingDetailsContentNode, items);
  heroMeetingDetailsSummaryNode.setAttribute(
    "aria-label",
    `Meeting details for ${meeting.title || "meeting"}`,
  );
  heroMeetingDetailsNode.hidden = false;
}

async function dismissEvent(eventId) {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) return;
  try {
    await invoke("dismiss_event", { eventId });
    await refreshDashboard();
    setStatus("Event dismissed");
  } catch (err) {
    setStatus(`Failed to dismiss: ${String(err)}`);
  }
}

async function clearAllDismissals() {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) return;
  try {
    await invoke("clear_dismissals");
    await refreshDashboard();
    setStatus("Cleared all dismissed events");
  } catch (err) {
    setStatus(`Failed to clear: ${String(err)}`);
  }
}

function renderMeetings(meetings) {
  meetingListNode.innerHTML = "";

  if (!meetings.length) {
    return;
  }

  for (const meeting of meetings.slice(0, 4)) {
    const card = document.createElement("article");
    card.className = "meeting";

    const header = meeting.joinUrl ? document.createElement("button") : document.createElement("div");
    header.className = meeting.joinUrl ? "meeting-header meeting-clickable" : "meeting-header";
    if (meeting.joinUrl) {
      header.type = "button";
      header.setAttribute("aria-label", `Join ${meeting.title}`);
    }

    const dayP = document.createElement("p");
    dayP.className = "meeting-day";
    dayP.textContent = formatMeetingDay(meeting);

    const titleH3 = document.createElement("h3");
    titleH3.className = "meeting-title";
    titleH3.textContent = meeting.title;

    const timeP = document.createElement("p");
    timeP.className = "meeting-time";
    timeP.textContent = formatMeetingRange(meeting);

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

    header.appendChild(dayP);
    header.appendChild(titleH3);
    header.appendChild(timeP);
    header.appendChild(serviceDiv);

    if (meeting.joinUrl) {
      header.addEventListener("click", async () => {
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

    card.appendChild(header);
    card.appendChild(createMeetingDetails(meeting));
    meetingListNode.appendChild(card);
  }
}

function renderUtilityActions() {
  const utilityActions = [
    { label: "Refresh calendars", command: "refresh_meetings", status: "Refreshing calendar data..." },
    { label: "Open calendar day", command: "open_calendar_day", status: "Opening your calendar..." },
    { label: "Clear dismissed events", action: clearAllDismissals },
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
      if (action.action) {
        await action.action();
        return;
      }
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
  const heroMeeting = ongoing || nextMeeting;
  renderHeroDetails(heroMeeting);

  if (ongoing) {
    heroTitleNode.textContent = `Live now: ${ongoing.title}`;
    const meta = `${formatMeetingRange(ongoing)} on ${ongoing.service}`;
    heroMetaNode.textContent = ongoing.relativeTime ? `${meta} \u00b7 ${ongoing.relativeTime}` : meta;
    joinNowButtonNode.textContent = joinButtonLabel(ongoing, "Join live");
    joinNowButtonNode.title = ongoing.title || "";
    joinNowButtonNode.setAttribute("aria-label", `Join live meeting: ${ongoing.title || "meeting"}`);
    joinNowButtonNode.style.display = "";
    return;
  }

  if (nextMeeting) {
    joinNowButtonNode.textContent = joinButtonLabel(nextMeeting, "Join next");
    joinNowButtonNode.title = nextMeeting.title || "";
    joinNowButtonNode.setAttribute("aria-label", `Join next meeting: ${nextMeeting.title || "meeting"}`);
    joinNowButtonNode.style.display = "";
    heroTitleNode.textContent = `Next: ${nextMeeting.title}`;
    const meta = `${formatMeetingRange(nextMeeting)} on ${nextMeeting.service}`;
    heroMetaNode.textContent = nextMeeting.relativeTime ? `${meta} \u00b7 ${nextMeeting.relativeTime}` : meta;
    return;
  }

  joinNowButtonNode.style.display = "none";
  joinNowButtonNode.title = "";
  joinNowButtonNode.removeAttribute("aria-label");
  heroTitleNode.textContent = "";
  heroMetaNode.textContent = "";
  clearHeroDetails();
}

function updateTitleFromMeetings(meetings) {
  if (!meetings.length) {
    todayTitleNode.textContent = "";
    return;
  }

  const start = meetingStartDate(meetings[0]);
  todayTitleNode.textContent = `${dayFormatter.format(start)} agenda`;
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

async function hideCurrentWindow() {
  const appWindow = window.__TAURI__?.window?.getCurrentWindow;
  if (!appWindow) {
    return;
  }
  await appWindow().hide();
}

function bindKeyboardShortcuts() {
  document.addEventListener("keydown", async (event) => {
    if (event.defaultPrevented) {
      return;
    }

    if (event.key === "Escape") {
      event.preventDefault();
      await hideCurrentWindow();
      return;
    }

    const hasCommandModifier = isMac ? event.metaKey : event.ctrlKey;
    if (!hasCommandModifier || event.altKey) {
      return;
    }

    const key = event.key.toLowerCase();
    if (key === "q") {
      event.preventDefault();
      await quitApp();
    } else if (key === "r") {
      event.preventDefault();
      await runCommand("refresh_meetings", "Refreshing calendar data...");
    } else if (key === ",") {
      event.preventDefault();
      await runCommand("open_preferences", "Opening preferences...");
    }
  });
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
    clearHeroDetails();
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
  bindKeyboardShortcuts();
  renderUtilityActions();

  showLoading();
  const dashboard = await loadDashboard();
  hideLoading();
  applyDashboard(dashboard);
  setStatus("");

  setInterval(refreshDashboard, REFRESH_INTERVAL_MS);
}

main();
