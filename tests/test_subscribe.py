import asyncio
import threading
from datetime import datetime, timedelta
from pathlib import Path
from typing import Optional

from nextmeeting.server import RpcServer


def _tsv_line(
    title: str, start: datetime, end: datetime, meet: Optional[str] = None
) -> str:
    meet = meet or ""
    return f"{start:%Y-%m-%d}\t{start:%H:%M}\t{end:%Y-%m-%d}\t{end:%H:%M}\thttps://cal\t{meet}\t{title}\n"


def _start_server(socket_path: str, texts: list[str], poll_interval: float = 0.05):
    ready = threading.Event()
    stopped = threading.Event()

    class Runner:
        loop: Optional[asyncio.AbstractEventLoop] = None
        server: Optional[RpcServer] = None

    run = Runner()

    def fetcher(_cal: Optional[str]) -> str:
        # return last value; pop to advance
        if texts:
            return texts[0]
        return ""

    def _thread_target() -> None:
        run.loop = asyncio.new_event_loop()
        asyncio.set_event_loop(run.loop)
        run.server = RpcServer(
            socket_path=socket_path,
            poll_interval=poll_interval,
            calendar=None,
            fetch_func=fetcher,
        )
        run.loop.run_until_complete(run.server.start())
        ready.set()
        try:
            run.loop.run_forever()
        finally:
            if run.server is not None:
                run.loop.run_until_complete(run.server.close())
            run.loop.close()
            stopped.set()

    th = threading.Thread(target=_thread_target, daemon=True)
    th.start()
    ready.wait(5)
    assert ready.is_set(), "server did not start"

    def advance_one():
        if texts:
            texts.pop(0)

    def stop() -> None:
        assert run.loop is not None
        if run.server is not None:
            fut = asyncio.run_coroutine_threadsafe(run.server.close(), run.loop)
            fut.result(timeout=5)
        run.loop.call_soon_threadsafe(run.loop.stop)
        stopped.wait(5)

    return advance_one, stop


def test_subscribe_next_changed(tmp_path: Path):
    now = datetime.now().replace(second=0, microsecond=0)
    a1 = _tsv_line("A", now + timedelta(minutes=2), now + timedelta(minutes=32))
    b1 = _tsv_line("B", now + timedelta(minutes=5), now + timedelta(minutes=35))
    texts = [a1, b1]
    sock = str(tmp_path / "sock")
    advance, stop = _start_server(sock, texts)
    got_event = False
    try:

        async def _go():
            nonlocal got_event
            reader, writer = await asyncio.open_unix_connection(sock)
            try:
                # subscribe
                writer.write(
                    b'{"id":"1","method":"subscribe","params":{"topics":["next"]}}\n'
                )
                await writer.drain()
                _ = await reader.readline()  # ack
                # Advance to trigger change
                advance()
                # Wait for an event
                for _ in range(50):
                    line = await reader.readline()
                    if not line:
                        break
                    if b"next_changed" in line:
                        got_event = True
                        break
            finally:
                writer.close()
                await writer.wait_closed()

        asyncio.run(_go())
    finally:
        stop()
    assert got_event
