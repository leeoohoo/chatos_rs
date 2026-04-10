from __future__ import annotations

from typing import Any, Callable, Protocol

from gateway_response.builder import build_non_stream_response_body
from gateway_base.types import TurnResult


class CreateResponseStore(Protocol):
    def put(self, response_id: str, thread_id: str) -> None: ...


def finalize_create_response(
    *,
    store: CreateResponseStore,
    result: TurnResult,
    model_name: str,
    previous_response_id: str | None,
    response_tools: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
    created_at: int,
    response_id_factory: Callable[[], str],
    message_id_factory: Callable[[], str],
    function_item_id_factory: Callable[[], str],
) -> tuple[str, dict[str, Any]]:
    response_id = response_id_factory()
    message_id = message_id_factory()

    store.put(response_id, result.thread_id)
    body = build_non_stream_response_body(
        response_id=response_id,
        model_name=model_name,
        result=result,
        previous_response_id=previous_response_id,
        response_tools=response_tools,
        provided_tool_outputs=provided_tool_outputs,
        message_id=message_id,
        created_at=created_at,
        function_item_id_factory=function_item_id_factory,
    )
    return response_id, body
