#!/usr/bin/env python
# Author: Chmouel Boudjnah <chmouel@chmouel.com>

from datetime import datetime, timedelta, timezone
from uuid import uuid4

from caldav import DAVClient
from icalendar import Calendar, Event

client = DAVClient(
    url="http://localhost:5232/",
    username="username",
    password="password",
)
principal = client.principal()
calendar = (
    client.calendar(url="http://localhost:5232/username/calendar/")
    or principal.calendars()[0]
)

now = datetime.now(timezone.utc)


def create_event(summary, start, end, url=None, description=None):
    cal = Calendar()
    cal.add("prodid", "-//nextmeeting demo//")
    cal.add("version", "2.0")

    event = Event()
    event.add("uid", f"{uuid4()}@nextmeeting")
    event.add("summary", summary)
    event.add("dtstart", start)
    event.add("dtend", end)
    if url:
        event.add("url", url)
    if description:
        event.add("description", description)
    cal.add_component(event)

    calendar.save_event(cal.to_ical().decode("utf-8"))


create_event(
    "Project Kickoff Meeting",
    now.replace(hour=9, minute=0, second=0, microsecond=0),
    now.replace(hour=10, minute=0, second=0, microsecond=0),
    url="https://meet.example.com/kickoff",
    description="Discuss project goals, deliverables, and assign initial tasks. Attendees: Alice, Bob, Carol.",
)
create_event(
    "Design Review",
    now.replace(hour=11, minute=0, second=0, microsecond=0),
    now.replace(hour=12, minute=0, second=0, microsecond=0),
    url="https://meet.example.com/design",
    description="Review UI/UX mockups and finalize design decisions. Attendees: Design team, Product manager.",
)
create_event(
    "Sprint Planning",
    now.replace(hour=13, minute=0, second=0, microsecond=0),
    now.replace(hour=14, minute=30, second=0, microsecond=0),
    url="https://meet.example.com/sprint",
    description="Plan tasks for the upcoming sprint, estimate stories, and assign responsibilities. Attendees: Dev team, Scrum master.",
)
create_event(
    "Client Demo",
    now.replace(hour=15, minute=0, second=0, microsecond=0),
    now.replace(hour=16, minute=0, second=0, microsecond=0),
    url="https://meet.example.com/clientdemo",
    description="Demonstrate current progress to client, gather feedback, and discuss next steps. Attendees: Client, Project manager.",
)
create_event(
    "Retrospective",
    now.replace(hour=17, minute=0, second=0, microsecond=0),
    now.replace(hour=18, minute=0, second=0, microsecond=0),
    url="https://meet.example.com/retro",
    description="Team retrospective to discuss what went well, what could be improved, and action items. Attendees: All team members.",
)
print("Created five detailed demo events for today.")

create_event(
    "CalDAV Demo",
    now + timedelta(hours=1),
    now + timedelta(hours=2),
    url="https://meet.example.com/demo",
)
create_event(
    "All Day Sync",
    (now + timedelta(days=1)).date(),
    (now + timedelta(days=2)).date(),
    description="Join via https://example.com/all",
)
print("Created two demo events.")
