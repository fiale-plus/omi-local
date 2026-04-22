from datetime import datetime, timezone
from typing import List, Dict, Any, Optional
import os

# LOCAL_MODE: SQLite-only, no Firestore
_LOCAL_MODE = os.getenv("LOCAL_MODE", "").lower() in ("1", "true", "yes")

if _LOCAL_MODE:
    SCREEN_ACTIVITY_COLLECTION = "screen_activity"
    USERS_COLLECTION = "users"

    def upsert_screen_activity(uid: str, rows: List[Dict[str, Any]]) -> int:
        raise NotImplementedError("cloud-only")

    def get_screen_activity_for_day(uid: str, date: str) -> List[Dict[str, Any]]:
        return []

    def get_screen_activity_count(uid: str) -> int:
        return 0

else:
    from google.cloud import firestore
    from ._client import db
    import logging

    logger = logging.getLogger(__name__)

    SCREEN_ACTIVITY_COLLECTION = "screen_activity"
    USERS_COLLECTION = "users"

    def upsert_screen_activity(uid: str, rows: List[Dict[str, Any]]) -> int:
        user_ref = db.collection("users").document(uid)
        screen_activity_ref = user_ref.collection(SCREEN_ACTIVITY_COLLECTION)

        count = 0
        for row in rows:
            doc_id = row.get("id")
            if doc_id:
                screen_activity_ref.document(doc_id).set(row, merge=True)
                count += 1
        return count

    def get_screen_activity_for_day(uid: str, date: str) -> List[Dict[str, Any]]:
        user_ref = db.collection("users").document(uid)
        screen_activity_ref = user_ref.collection(SCREEN_ACTIVITY_COLLECTION)
        query = screen_activity_ref.where("date", "==", date)
        return [doc.to_dict() for doc in query.stream()]

    def get_screen_activity_count(uid: str) -> int:
        user_ref = db.collection("users").document(uid)
        screen_activity_ref = user_ref.collection(SCREEN_ACTIVITY_COLLECTION)
        count = 0
        for _ in screen_activity_ref.stream():
            count += 1
        return count
