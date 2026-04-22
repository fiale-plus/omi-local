"""Stub for airgap/LOCAL_MODE=1 — Pusher WebSocket calls are not available."""
import logging
import asyncio

logger = logging.getLogger(__name__)


class PusherCircuitBreakerOpen(Exception):
    """Raised when the circuit breaker is open and rejecting connections."""
    pass


class PusherCircuitBreaker:
    """Stub circuit breaker for airgap mode."""

    def __init__(self, failure_threshold=20, failure_window=30.0, cooldown=60.0):
        pass

    @property
    def state(self):
        from enum import Enum
        class CircuitState(str, Enum):
            OPEN = 'open'
        return CircuitState.OPEN

    def can_attempt(self):
        return False

    def record_failure(self):
        pass

    def record_success(self):
        pass

    def acquire_probe(self):
        return False


def get_circuit_breaker():
    return PusherCircuitBreaker()


async def connect_to_trigger_pusher(uid: str, sample_rate: int = 8000, retries: int = 3, is_active: callable = None):
    """No-op stub for airgap — raises immediately."""
    raise PusherCircuitBreakerOpen(f"LOCAL_MODE=1: connect_to_trigger_pusher unavailable for {uid}")


async def _connect_to_trigger_pusher(uid: str, sample_rate: int = 8000):
    """No-op stub for airgap."""
    raise PusherCircuitBreakerOpen(f"LOCAL_MODE=1: _connect_to_trigger_pusher unavailable for {uid}")
