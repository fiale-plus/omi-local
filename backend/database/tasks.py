import os

# LOCAL_MODE: no Firestore
_LOCAL_MODE = os.getenv("LOCAL_MODE", "").lower() in ("1", "true", "yes")

if _LOCAL_MODE:
    # Stub: all functions are cloud-only
    def create(task_data: dict):
        raise NotImplementedError("cloud-only")

    def update(task_id: str, task_data: dict):
        raise NotImplementedError("cloud-only")

    def get_task_by_action_request(action: str, request_id: str):
        raise NotImplementedError("cloud-only")
else:
    from google.cloud.firestore_v1 import FieldFilter

    from ._client import db

    def create(task_data: dict):
        task_id = task_data['id']
        task_ref = db.collection('tasks').document(task_id)
        task_ref.set(task_data)

    def update(task_id: str, task_data: dict):
        task_ref = db.collection('tasks').document(task_id)
        task_ref.update(task_data)

    def get_task_by_action_request(action: str, request_id: str):
        query = (
            db.collection('tasks')
            .where(filter=FieldFilter('action', '==', action))
            .where(filter=FieldFilter('request_id', '==', request_id))
            .limit(1)
        )
        tasks = [item.to_dict() for item in query.stream()]
        if len(tasks) > 0:
            return tasks[0]

        return None
