const timelineNode = document.querySelector("#timeline");
const meetingListNode = document.querySelector("#meetingList");
const actionListNode = document.querySelector("#actionList");
const sourceBadgeNode = document.querySelector("#sourceBadge");
const todayTitleNode = document.querySelector("#todayTitle");
const statusLineNode = document.querySelector("#statusLine");

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
    meetingListNode.innerHTML = '<p class="empty">No meetings right now.</p>';
    return;
  }

  meetingListNode.innerHTML = meetings
    .map(
      (meeting) => `
        <article class="meeting">
          <p class="meeting-day">${meeting.dayLabel}</p>
          <h2 class="meeting-title">${meeting.title}</h2>
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

function renderActions(actions) {
  actionListNode.innerHTML = actions
    .map((action) => `<button class="action" type="button">${action}</button>`)
    .join("");

  actionListNode.querySelectorAll(".action").forEach((button) => {
    button.addEventListener("click", async () => {
      const action = button.textContent;
      if (action === "Join next meeting") {
        await runCommand("join_next_meeting", "Opening your next meeting...");
        return;
      }
      if (action === "Create meeting") {
        await runCommand("create_meeting", "Creating a new meeting...");
        return;
      }
      if (action === "Quick Actions") {
        await runCommand("open_calendar_day", "Opening your calendar...");
        return;
      }
      if (action === "Preferences") {
        await runCommand("open_preferences", "Opening preferences...");
        return;
      }
      if (action === "Quit") {
        quitApp();
      }
    });
  });
}

function updateTitleFromMeetings(meetings) {
  if (!meetings.length) {
    todayTitleNode.textContent = "Today";
    return;
  }

  todayTitleNode.textContent = `Today (${meetings[0].dayLabel})`;
}

function fallbackData() {
  return {
    source: "unavailable",
    meetings: [],
    actions: [
      "Join next meeting",
      "Create meeting",
      "Quick Actions",
      "Preferences",
      "Quit",
    ],
  };
}

function setStatus(message) {
  statusLineNode.textContent = message;
}

async function joinNextMeeting() {
  await runCommand("join_next_meeting", "Opening your next meeting...");
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

async function main() {
  const dashboard = await loadDashboard();
  sourceBadgeNode.textContent = dashboard.source;
  renderTimeline(dashboard.meetings || []);
  renderMeetings(dashboard.meetings || []);
  renderActions(dashboard.actions || []);
  updateTitleFromMeetings(dashboard.meetings || []);
  setStatus("");
}

main();
