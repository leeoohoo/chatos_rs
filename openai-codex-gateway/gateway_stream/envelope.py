from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable

from gateway_response.builder import build_stream_response_object


ResponseObjFactory = Callable[..., dict[str, Any]]


@dataclass
class StreamEnvelopeSetup:
    response_id: str
    created_at: int
    response_obj: ResponseObjFactory


def build_stream_envelope_setup(
    *,
    response_id: str,
    created_at: int,
    model_name: str,
    response_tools: list[dict[str, Any]],
) -> StreamEnvelopeSetup:
    def response_obj(
        *,
        status: str,
        output: list[dict[str, Any]],
        usage: dict[str, Any] | None = None,
        error: dict[str, Any] | None = None,
        reasoning: str | None = None,
        previous_response_id: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        return build_stream_response_object(
            response_id=response_id,
            created_at=created_at,
            model_name=model_name,
            response_tools=response_tools,
            status=status,
            output=output,
            usage=usage,
            error=error,
            reasoning=reasoning,
            previous_response_id=previous_response_id,
            metadata=metadata,
        )

    return StreamEnvelopeSetup(
        response_id=response_id,
        created_at=created_at,
        response_obj=response_obj,
    )
