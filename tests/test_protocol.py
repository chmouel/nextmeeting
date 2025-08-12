import http

from nextmeeting.protocol import Error, Request, Response


def test_request_response_roundtrip():
    req = Request(id="abc", method="ping", params={})
    line = req.to_json_line()
    parsed = Request.from_json_line(line)
    assert parsed.id == "abc"
    assert parsed.method == "ping"
    assert parsed.params == {}

    resp = Response(id="abc", result={"ok": True})
    rline = resp.to_json_line()
    rparsed = Response.from_json_line(rline)
    assert rparsed.id == "abc"
    assert rparsed.error is None
    assert rparsed.result == {"ok": True}

    err = Response(id="abc", error=Error(code=400, message="bad"))
    eline = err.to_json_line()
    eparsed = Response.from_json_line(eline)
    assert eparsed.error is not None
    assert eparsed.error.code == http.HTTPStatus.BAD_REQUEST.value
    assert eparsed.error.message == "bad"
