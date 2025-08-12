from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any, Dict, Optional


@dataclass
class Request:
    id: str
    method: str
    params: Dict[str, Any]

    def to_json_line(self) -> bytes:
        return (
            json.dumps({"id": self.id, "method": self.method, "params": self.params})
            + "\n"
        ).encode("utf-8")

    @staticmethod
    def from_json_line(line: bytes) -> "Request":
        obj = json.loads(line.decode("utf-8"))
        if not isinstance(obj, dict):
            raise ValueError("invalid request payload")
        return Request(
            id=str(obj.get("id")),
            method=str(obj.get("method")),
            params=obj.get("params") or {},
        )


@dataclass
class Error:
    code: int
    message: str


@dataclass
class Response:
    id: str
    result: Optional[Any] = None
    error: Optional[Error] = None

    def to_json_line(self) -> bytes:
        payload: Dict[str, Any] = {"id": self.id}
        if self.error is not None:
            payload["error"] = {"code": self.error.code, "message": self.error.message}
        else:
            payload["result"] = self.result
        return (json.dumps(payload) + "\n").encode("utf-8")

    @staticmethod
    def from_json_line(line: bytes) -> "Response":
        obj = json.loads(line.decode("utf-8"))
        if not isinstance(obj, dict):
            raise ValueError("invalid response payload")
        err = obj.get("error")
        error_obj = (
            Error(code=int(err["code"]), message=str(err["message"])) if err else None
        )
        return Response(
            id=str(obj.get("id")), result=obj.get("result"), error=error_obj
        )
