from datetime import datetime, timedelta

from nextmeeting.core import (
    Meeting,
    parse_tsv,
    apply_filters,
    FilterOptions,
    compute_next,
)


def test_parse_and_sort_simple():
    now = datetime.now()
    start = now.replace(hour=10, minute=0, second=0, microsecond=0)
    end = start + timedelta(minutes=30)
    line = f"{start:%Y-%m-%d}\t{start:%H:%M}\t{end:%Y-%m-%d}\t{end:%H:%M}\thttps://cal\thttps://meet\tTitle"
    meetings = parse_tsv(line)
    assert len(meetings) == 1
    m = meetings[0]
    assert m.title == "Title"
    assert m.meet_url == "https://meet"


def test_filters_and_next():
    now = datetime.now().replace(second=0, microsecond=0)
    m1 = Meeting(
        "A", now + timedelta(minutes=60), now + timedelta(minutes=90), "https://cal"
    )
    m2 = Meeting(
        "B",
        now + timedelta(minutes=10),
        now + timedelta(minutes=40),
        "https://cal",
        "https://link",
    )
    f = FilterOptions(only_with_link=True, within_mins=30)
    out = apply_filters([m1, m2], f)
    assert out
    assert out[0].title == "B"
    nxt = compute_next(out)
    assert nxt
    assert nxt.title == "B"
