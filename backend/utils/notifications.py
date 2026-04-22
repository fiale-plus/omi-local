"""Stub for airgap/LOCAL_MODE=1 — FCM push notifications are not available."""
import logging

logger = logging.getLogger(__name__)

# Stubbed for LOCAL_MODE=1 airgap deployment (Firebase Cloud Messaging unavailable)


def send_notification(user_id: str, title: str, body: str, data: dict = None, tokens: list = None):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_notification suppressed for user {user_id}")


def send_subscription_paid_personalized_notification(user_id: str, data: dict = None):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_subscription_paid_personalized_notification suppressed for user {user_id}")


def send_credit_limit_notification(user_id: str):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_credit_limit_notification suppressed for user {user_id}")


def send_silent_user_notification(user_id: str):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_silent_user_notification suppressed for user {user_id}")


def send_training_data_submitted_notification(user_id: str):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_training_data_submitted_notification suppressed for user {user_id}")


async def send_bulk_notification(user_tokens: list, title: str, body: str):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_bulk_notification suppressed")


def send_app_review_reply_notification(reviewer_uid: str, app_owner_uid: str, reply_body: str, app_id: str, app_name: str):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_app_review_reply_notification suppressed")


def send_new_app_review_notification(app_owner_uid: str, reviewer_uid: str, app_id: str, app_name: str, review_body: str):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_new_app_review_notification suppressed")


def send_action_item_data_message(user_id: str, action_item_id: str, description: str, due_at: str):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_action_item_data_message suppressed")


def send_apple_reminders_sync_push(user_id: str, action_items: list) -> bool:
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_apple_reminders_sync_push suppressed")
    return False


def send_merge_completed_message(user_id: str, merged_conversation_id: str, removed_conversation_ids: list):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_merge_completed_message suppressed")


def send_important_conversation_message(user_id: str, conversation_id: str):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_important_conversation_message suppressed")


def send_action_item_update_message(user_id: str, action_item_id: str, description: str, due_at: str):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_action_item_update_message suppressed")


def send_action_item_deletion_message(user_id: str, action_item_id: str):
    """No-op stub for airgap."""
    logger.debug(f"LOCAL_MODE=1: send_action_item_deletion_message suppressed")
