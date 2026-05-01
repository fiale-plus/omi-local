import uuid
from datetime import datetime, timezone
from typing import List, Optional, Dict, Any
import os

# LOCAL_MODE: SQLite-only, no Firestore
_LOCAL_MODE = os.getenv("LOCAL_MODE", "").lower() in ("1", "true", "yes")

if _LOCAL_MODE:
    SYSTEM_FOLDERS = [
        {"name": "Work", "category_mapping": "work"},
        {"name": "Personal", "category_mapping": "personal"},
        {"name": "Health", "category_mapping": "health"},
    ]

    def create_folder(uid: str, folder_data: Dict[str, Any]):
        raise NotImplementedError("cloud-only")

    def get_folder(uid: str, folder_id: str):
        return None

    def get_folders(uid: str, limit: int = 100, offset: int = 0):
        return []

    def update_folder(uid: str, folder_id: str, updates: Dict[str, Any]):
        raise NotImplementedError("cloud-only")

    def delete_folder(uid: str, folder_id: str):
        raise NotImplementedError("cloud-only")

    def create_system_folders(uid: str):
        raise NotImplementedError("cloud-only")

    def get_folders_by_category(uid: str, category: str):
        return []

    def get_default_folders(uid: str):
        return []

else:
    from google.cloud import firestore
    from google.cloud.firestore_v1 import FieldFilter
    from ._client import db
    from models.folder import Folder

    SYSTEM_FOLDERS = [
        {"name": "Work", "category_mapping": "work"},
        {"name": "Personal", "category_mapping": "personal"},
        {"name": "Health", "category_mapping": "health"},
    ]

    def create_folder(uid: str, folder_data: Dict[str, Any]):
        folders_ref = db.collection("users").document(uid).collection("folders")
        folder_id = folder_data.get("id", str(uuid.uuid4()))
        folder_data["id"] = folder_id
        folders_ref.document(folder_id).set(folder_data)
        return folder_id

    def get_folder(uid: str, folder_id: str):
        folder_ref = db.collection("users").document(uid).collection("folders").document(folder_id)
        doc = folder_ref.get()
        if doc.exists:
            return doc.to_dict()
        return None

    def get_folders(uid: str, limit: int = 100, offset: int = 0):
        folders_ref = db.collection("users").document(uid).collection("folders")
        return [doc.to_dict() for doc in folders_ref.limit(limit).offset(offset).stream()]

    def update_folder(uid: str, folder_id: str, updates: Dict[str, Any]):
        folder_ref = db.collection("users").document(uid).collection("folders").document(folder_id)
        folder_ref.update(updates)

    def delete_folder(uid: str, folder_id: str):
        folder_ref = db.collection("users").document(uid).collection("folders").document(folder_id)
        folder_ref.delete()

    def create_system_folders(uid: str):
        for system_folder in SYSTEM_FOLDERS:
            create_folder(uid, system_folder)

    def get_folders_by_category(uid: str, category: str):
        folders_ref = db.collection("users").document(uid).collection("folders")
        query = folders_ref.where(filter=FieldFilter("category_mapping", "==", category))
        return [doc.to_dict() for doc in query.stream()]

    def get_default_folders(uid: str):
        return SYSTEM_FOLDERS
