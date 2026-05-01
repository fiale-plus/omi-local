"""
Daily Summaries database module

Structure:
users/{uid}/daily_summaries/{summary_id}
"""
import uuid
from datetime import datetime, timezone
from typing import List, Dict, Any, Optional
import os

# LOCAL_MODE: SQLite-only, no Firestore
_LOCAL_MODE = os.getenv("LOCAL_MODE", "").lower() in ("1", "true", "yes")

if _LOCAL_MODE:
    def create_daily_summary(uid: str, summary_data: Dict[str, Any]):
        raise NotImplementedError("cloud-only")

    def get_daily_summary(uid: str, summary_id: str):
        return None

    def get_daily_summaries(uid: str, limit: int = 100, offset: int = 0):
        return []

    def get_daily_summaries_by_date_range(
        uid: str, start_date: str, end_date: str, limit: int = 100
    ):
        return []

    def update_daily_summary(uid: str, summary_id: str, updates: Dict[str, Any]):
        raise NotImplementedError("cloud-only")

    def delete_daily_summary(uid: str, summary_id: str):
        raise NotImplementedError("cloud-only")

else:
    from google.cloud import firestore
    from google.cloud.firestore_v1 import FieldFilter
    from ._client import db

    def create_daily_summary(uid: str, summary_data: Dict[str, Any]):
        summaries_ref = db.collection("users").document(uid).collection("daily_summaries")
        summary_id = summary_data.get("id", str(uuid.uuid4()))
        summary_data["id"] = summary_id
        summaries_ref.document(summary_id).set(summary_data)
        return summary_id

    def get_daily_summary(uid: str, summary_id: str):
        summary_ref = db.collection("users").document(uid).collection("daily_summaries").document(summary_id)
        doc = summary_ref.get()
        if doc.exists:
            return doc.to_dict()
        return None

    def get_daily_summaries(uid: str, limit: int = 100, offset: int = 0):
        summaries_ref = db.collection("users").document(uid).collection("daily_summaries")
        return [doc.to_dict() for doc in summaries_ref.order_by("date", direction="DESC").limit(limit).offset(offset).stream()]

    def get_daily_summaries_by_date_range(
        uid: str, start_date: str, end_date: str, limit: int = 100
    ):
        summaries_ref = db.collection("users").document(uid).collection("daily_summaries")
        query = summaries_ref.where("date", ">=", start_date).where("date", "<=", end_date).limit(limit)
        return [doc.to_dict() for doc in query.stream()]

    def update_daily_summary(uid: str, summary_id: str, updates: Dict[str, Any]):
        summary_ref = db.collection("users").document(uid).collection("daily_summaries").document(summary_id)
        summary_ref.update(updates)

    def delete_daily_summary(uid: str, summary_id: str):
        summary_ref = db.collection("users").document(uid).collection("daily_summaries").document(summary_id)
        summary_ref.delete()
