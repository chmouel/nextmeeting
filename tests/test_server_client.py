import asyncio
import threading
from typing import Optional
from pathlib import Path

from nextmeeting.client import _rpc_call
from nextmeeting.server import RpcServer


def _start_server_in_thread(socket_path: str):
    ready = threading.Event()
    stopped = threading.Event()

    class Runner:
        loop: Optional[asyncio.AbstractEventLoop] = None
        server: Optional[RpcServer] = None

    run = Runner()

    def _thread_target() -> None:
        run.loop = asyncio.new_event_loop()
        asyncio.set_event_loop(run.loop)
        run.server = RpcServer(socket_path=socket_path)
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
    assert ready.is_set(), "server did not start in time"

    def stop() -> None:
        assert run.loop is not None
        # Ensure server is closed before stopping loop
        if run.server is not None:
            fut = asyncio.run_coroutine_threadsafe(run.server.close(), run.loop)
            fut.result(timeout=5)
        run.loop.call_soon_threadsafe(run.loop.stop)
        stopped.wait(5)

    return stop


def test_ping_roundtrip(tmp_path: Path):
    sock = tmp_path / "sock"
    stop = _start_server_in_thread(str(sock))
    try:

        async def _go():
            resp = await _rpc_call(str(sock), "ping", {})
            assert resp.error is None
            assert resp.result == "pong"

        asyncio.run(_go())
    finally:
        stop()
