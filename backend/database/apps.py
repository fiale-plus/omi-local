import os
from datetime import datetime, timezone
from typing import List

_LOCAL_MODE = os.getenv("LOCAL_MODE", "").lower() in ("1", "true", "yes")

if _LOCAL_MODE:
    apps_collection = 'plugins_data'
    app_analytics_collection = 'plugins'
    testers_collection = 'testers'

    def get_app_by_id_db(app_id: str):
        return None

    def record_app_usage(uid: str, app_id: str, usage_type: str):
        pass

    def get_audio_apps_count(app_ids: List[str]):
        return 0

    def get_private_apps_db(uid: str) -> List:
        return []

    def get_unapproved_public_apps_db() -> List:
        return []

    def get_public_approved_apps_db() -> List:
        return []

    def get_popular_apps_db() -> List:
        return []

    def set_app_popular_db(app_id: str, popular: bool):
        pass

    def search_apps_db(uid: str, category=None, capability=None, my_apps=False, installed_apps=False, enabled_app_ids=None) -> List:
        return []

    def get_public_unapproved_apps_db(uid: str) -> List:
        return []

    def get_apps_for_tester_db(uid: str) -> List:
        return []

    def add_app_to_db(app_data: dict):
        pass

    def upsert_app_to_db(app_data: dict):
        pass

    def update_app_in_db(app_data: dict):
        pass

    def delete_app_from_db(app_id: str):
        pass

    def update_app_visibility_in_db(app_id: str, private: bool):
        pass

    def change_app_approval_status(app_id: str, approved: bool):
        pass

    def get_app_usage_history_db(app_id: str):
        return []

    def get_app_memory_created_integration_usage_count_db(app_id: str):
        return 0

    def get_app_memory_prompt_usage_count_db(app_id: str):
        return 0

    def get_app_chat_message_sent_usage_count_db(app_id: str):
        return 0

    def get_app_usage_count_db(app_id: str):
        return 0

    def set_app_review_in_db(app_id: str, uid: str, review: dict):
        pass

    def add_tester_db(data: dict):
        pass

    def add_app_access_for_tester_db(app_id: str, uid: str):
        pass

    def remove_app_access_for_tester_db(app_id: str, uid: str):
        pass

    def remove_tester_db(uid: str):
        pass

    def can_tester_access_app_db(app_id: str, uid: str) -> bool:
        return False

    def is_tester_db(uid: str) -> bool:
        return False

    def delete_persona_db(persona_id: str):
        pass

    def get_personas_by_username_db(persona_id: str):
        return None

    def get_persona_by_username_db(username: str):
        return None

    def get_persona_by_id_db(persona_id: str):
        return None

    def get_persona_by_uid_db(uid: str):
        return None

    def get_user_persona_by_uid(uid: str):
        return None

    def get_persona_by_twitter_handle_db(handle: str):
        return None

    def get_persona_by_username_twitter_handle_db(username: str, handle: str):
        return None

    def get_omi_personas_by_uid_db(uid: str):
        return []

    def get_omi_persona_apps_by_uid_db(uid: str):
        return []

    def update_persona_in_db(persona_data: dict):
        pass

    def migrate_app_owner_id_db(new_id: str, old_id: str):
        pass

    def create_api_key_db(app_id: str, api_key_data: dict):
        return api_key_data

    def get_api_key_by_hash_db(app_id: str, hashed_key: str):
        return None

    def delete_api_key_db(app_id: str, api_key_id: str):
        pass

    def get_api_keys_for_app_db(app_id: str) -> List[dict]:
        return []

    def list_api_keys_db(app_id: str):
        return []

    def add_app_to_collection(app_id: str, collection_id: str):
        pass

    def remove_app_from_collection(app_id: str, collection_id: str):
        pass

    def get_collection_db(collection_id: str):
        return None

    def get_collections_for_user_db(uid: str) -> List[dict]:
        return []

    def create_collection_db(collection_data: dict) -> dict:
        return collection_data

    def update_collection_in_db(collection_id: str, updates: dict):
        pass

    def delete_collection_from_db(collection_id: str):
        pass

    def record_app_install(uid: str, app_id: str):
        pass

    def increment_app_installs_count(app_id: str):
        pass

    def record_app_uninstall(uid: str, app_id: str):
        pass

    def get_app_installs_count(app_id: str) -> int:
        return 0

else:
    from google.cloud.firestore_v1.base_query import BaseCompositeFilter, FieldFilter
    from google.cloud.firestore import ArrayUnion, ArrayRemove, firestore

    from ulid import ULID

    from models.app import UsageHistoryType
    from ._client import db
    import logging

    logger = logging.getLogger(__name__)

    # *****************************
    # ********** CRUD *************
    # *****************************

    apps_collection = 'plugins_data'
    app_analytics_collection = 'plugins'
    testers_collection = 'testers'


    def get_app_by_id_db(app_id: str):
        app_ref = db.collection(apps_collection).document(app_id)
        doc = app_ref.get()
        if doc.exists:
            return doc.to_dict()
        return None


    def get_audio_apps_count(app_ids: List[str]):
        if not app_ids or len(app_ids) == 0:
            return 0
        filters = [FieldFilter('id', 'in', app_ids), FieldFilter('external_integration.triggers_on', '==', 'audio_bytes')]
        apps_ref = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).count().get()
        return apps_ref[0][0].value


    def get_private_apps_db(uid: str) -> List:
        filters = [FieldFilter('uid', '==', uid), FieldFilter('private', '==', True)]
        private_apps = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).stream()
        data = [doc.to_dict() for doc in private_apps]
        return data


    # This returns public unapproved apps of all users
    def get_unapproved_public_apps_db() -> List:
        filters = [FieldFilter('approved', '==', False), FieldFilter('private', '==', False)]
        public_apps = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).stream()
        return [doc.to_dict() for doc in public_apps]


    def get_public_approved_apps_db() -> List:
        filters = [FieldFilter('approved', '==', True), FieldFilter('private', '==', False)]
        public_apps = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).stream()
        return [doc.to_dict() for doc in public_apps]


    def get_popular_apps_db() -> List:
        filters = [FieldFilter('approved', '==', True), FieldFilter('is_popular', '==', True)]
        popular_apps = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).stream()
        return [doc.to_dict() for doc in popular_apps]


    def set_app_popular_db(app_id: str, popular: bool):
        app_ref = db.collection(apps_collection).document(app_id)
        app_ref.update({'is_popular': popular})


    def search_apps_db(
        uid: str,
        category: str | None = None,
        capability: str | None = None,
        my_apps: bool = False,
        installed_apps: bool = False,
        enabled_app_ids: List[str] | None = None,
    ) -> List:
        """
        Optimized search function that applies filters at database level.
        Uses smart filter ordering to minimize data fetched from Firestore.

        Note: Rating filter is NOT applied here as rating_avg is calculated from Redis,
        not stored in Firestore. Apply rating filter after fetching from DB.
        """
        filters = []

        # 1. Apply most restrictive filter first
        if my_apps:
            filters.append(FieldFilter('uid', '==', uid))

        elif installed_apps:
            if not enabled_app_ids or len(enabled_app_ids) == 0:
                return []

            if len(enabled_app_ids) > 30:
                filters.append(FieldFilter('approved', '==', True))
                filters.append(FieldFilter('private', '==', False))
            else:
                filters.append(FieldFilter('id', 'in', enabled_app_ids))

        else:
            filters.append(FieldFilter('approved', '==', True))
            filters.append(FieldFilter('private', '==', False))

        if category and not my_apps:
            filters.append(FieldFilter('category', '==', category))

        if capability and not my_apps:
            filters.append(FieldFilter('capabilities', 'array_contains', capability))

        if filters:
            query = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters))
            apps = [doc.to_dict() for doc in query.stream()]
        else:
            apps = []

        if installed_apps and enabled_app_ids and len(enabled_app_ids) > 30:
            enabled_set = set(enabled_app_ids)
            apps = [app for app in apps if app.get('id') in enabled_set]

            user_apps_filter = FieldFilter('uid', '==', uid)
            user_apps_query = db.collection(apps_collection).where(filter=user_apps_filter)
            user_apps = [doc.to_dict() for doc in user_apps_query.stream()]

            existing_ids = {app.get('id') for app in apps}
            for user_app in user_apps:
                if user_app.get('id') in enabled_set and user_app.get('id') not in existing_ids:
                    apps.append(user_app)

        if my_apps and category:
            apps = [app for app in apps if app.get('category') == category]

        if my_apps and capability:
            apps = [app for app in apps if capability in app.get('capabilities', [])]

        return apps


    # This returns public unapproved apps for a user
    def get_public_unapproved_apps_db(uid: str) -> List:
        filters = [FieldFilter('approved', '==', False), FieldFilter('uid', '==', uid), FieldFilter('private', '==', False)]
        public_apps = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).stream()
        return [doc.to_dict() for doc in public_apps]


    def get_apps_for_tester_db(uid: str) -> List:
        tester_ref = db.collection(testers_collection).document(uid)
        doc = tester_ref.get()
        if doc.exists:
            apps = doc.to_dict().get('apps', [])
            if not apps:
                return []
            filters = [FieldFilter('approved', '==', False), FieldFilter('id', 'in', apps)]
            public_apps = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).stream()
            return [doc.to_dict() for doc in public_apps]
        return []


    def add_app_to_db(app_data: dict):
        app_ref = db.collection(apps_collection)
        app_ref.add(app_data, app_data['id'])


    def upsert_app_to_db(app_data: dict):
        app_ref = db.collection(apps_collection).document(app_data['id'])
        app_ref.set(app_data)


    def update_app_in_db(app_data: dict):
        app_ref = db.collection(apps_collection).document(app_data['id'])
        app_ref.update(app_data)


    def delete_app_from_db(app_id: str):
        app_ref = db.collection(apps_collection).document(app_id)
        app_ref.delete()


    def update_app_visibility_in_db(app_id: str, private: bool):
        app_ref = db.collection(apps_collection).document(app_id)
        if 'private' in app_id and not private:
            app = app_ref.get().to_dict()
            app_ref.delete()
            new_app_id = app_id.split('-private')[0] + '-' + str(ULID())
            app['id'] = new_app_id
            app['private'] = private
            app_ref = db.collection(apps_collection).document(new_app_id)
            app_ref.set(app)
        else:
            app_ref.update({'private': private})


    def change_app_approval_status(app_id: str, approved: bool):
        app_ref = db.collection(apps_collection).document(app_id)
        app_ref.update({'approved': approved, 'status': 'approved' if approved else 'rejected'})


    def get_app_usage_history_db(app_id: str):
        usage = db.collection(app_analytics_collection).document(app_id).collection('usage_history').stream()
        return [doc.to_dict() for doc in usage]


    def get_app_memory_created_integration_usage_count_db(app_id: str):
        usage = (
            db.collection(app_analytics_collection)
            .document(app_id)
            .collection('usage_history')
            .where(filter=FieldFilter('type', '==', UsageHistoryType.memory_created_external_integration))
            .count()
            .get()
        )
        return usage[0][0].value


    def get_app_memory_prompt_usage_count_db(app_id: str):
        usage = (
            db.collection(app_analytics_collection)
            .document(app_id)
            .collection('usage_history')
            .where(filter=FieldFilter('type', '==', UsageHistoryType.memory_created_prompt))
            .count()
            .get()
        )
        return usage[0][0].value


    def get_app_chat_message_sent_usage_count_db(app_id: str):
        usage = (
            db.collection(app_analytics_collection)
            .document(app_id)
            .collection('usage_history')
            .where(filter=FieldFilter('type', '==', UsageHistoryType.chat_message_sent))
            .count()
            .get()
        )
        return usage[0][0].value


    def get_app_usage_count_db(app_id: str):
        usage = db.collection(app_analytics_collection).document(app_id).collection('usage_history').count().get()
        return usage[0][0].value


    # ********************************
    # *********** REVIEWS ************
    # ********************************


    def set_app_review_in_db(app_id: str, uid: str, review: dict):
        app_ref = db.collection(apps_collection).document(app_id).collection('reviews').document(uid)
        app_ref.set(review)


    # ********************************
    # ************ TESTER ************
    # ********************************


    def add_tester_db(data: dict):
        app_ref = db.collection(testers_collection).document(data['uid'])
        app_ref.set(data)


    def add_app_access_for_tester_db(app_id: str, uid: str):
        app_ref = db.collection(testers_collection).document(uid)
        app_ref.update({'apps': ArrayUnion([app_id])})


    def remove_app_access_for_tester_db(app_id: str, uid: str):
        app_ref = db.collection(testers_collection).document(uid)
        app_ref.update({'apps': ArrayRemove([app_id])})


    def remove_tester_db(uid: str):
        app_ref = db.collection(testers_collection).document(uid)
        app_ref.delete()


    def can_tester_access_app_db(app_id: str, uid: str) -> bool:
        app_ref = db.collection(testers_collection).document(uid)
        doc = app_ref.get()
        if doc.exists:
            return app_id in doc.to_dict().get('apps', [])
        return False


    def is_tester_db(uid: str) -> bool:
        app_ref = db.collection(testers_collection).document(uid)
        return app_ref.get().exists


    # ********************************
    # *********** APPS USAGE *********
    # ********************************


    def record_app_usage(
        uid: str,
        app_id: str,
        usage_type: UsageHistoryType,
        conversation_id: str = None,
        message_id: str = None,
        timestamp: datetime = None,
    ):
        if not conversation_id and not message_id:
            raise ValueError('memory_id or message_id must be provided')

        data = {
            'uid': uid,
            'memory_id': conversation_id,
            'message_id': message_id,
            'timestamp': datetime.now(timezone.utc) if timestamp is None else timestamp,
            'type': usage_type,
        }

        db.collection(app_analytics_collection).document(app_id).collection('usage_history').document(
            conversation_id or message_id
        ).set(data)
        return data


    # ********************************
    # *********** PERSONAS ***********
    # ********************************


    def delete_persona_db(persona_id: str):
        persona_ref = db.collection(apps_collection).document(persona_id)
        persona_ref.delete()


    def get_personas_by_username_db(persona_id: str):
        persona_ref = db.collection(apps_collection).where('username', '==', persona_id)
        docs = persona_ref.get()
        if not docs:
            return None
        return [{**doc.to_dict(), 'doc_id': doc.id} for doc in docs]


    def get_persona_by_username_db(username: str):
        filters = [FieldFilter('username', '==', username), FieldFilter('capabilities', 'array_contains', 'persona')]
        persona_ref = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).limit(1)
        docs = persona_ref.get()
        if not docs:
            return None
        doc = next(iter(docs), None)
        if not doc:
            return None
        return doc.to_dict()


    def get_persona_by_id_db(persona_id: str):
        persona_ref = db.collection(apps_collection).document(persona_id)
        doc = persona_ref.get()
        if doc.exists:
            return doc.to_dict()
        return None


    def get_persona_by_uid_db(uid: str):
        filters = [FieldFilter('uid', '==', uid), FieldFilter('capabilities', 'array_contains', 'persona')]
        persona_ref = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).limit(1)
        docs = persona_ref.get()
        if not docs:
            return None
        doc = next(iter(docs), None)
        if not doc:
            return None
        return doc.to_dict()


    def get_user_persona_by_uid(uid: str):
        filters = [
            FieldFilter('capabilities', 'array_contains', 'persona'),
            FieldFilter('category', '==', 'personality-emulation'),
            FieldFilter('uid', '==', uid),
        ]
        persona_ref = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).limit(1)
        docs = persona_ref.get()
        if not docs:
            return None
        doc = next(iter(docs), None)
        if not doc:
            return None
        return {'id': doc.id, **doc.to_dict()}


    def get_persona_by_twitter_handle_db(handle: str):
        filters = [FieldFilter('category', '==', 'personality-emulation'), FieldFilter('twitter.username', '==', handle)]
        persona_ref = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).limit(1)
        docs = persona_ref.get()
        if not docs:
            return None
        doc = next(iter(docs), None)
        if not doc:
            return None
        return {'id': doc.id, **doc.to_dict()}


    def get_persona_by_username_twitter_handle_db(username: str, handle: str):
        filters = [
            FieldFilter('username', '==', username),
            FieldFilter('category', '==', 'personality-emulation'),
            FieldFilter('twitter.username', '==', handle),
        ]
        persona_ref = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).limit(1)
        docs = persona_ref.get()
        if not docs:
            return None
        doc = next(iter(docs), None)
        if not doc:
            return None
        return {'id': doc.id, **doc.to_dict()}


    def get_omi_personas_by_uid_db(uid: str):
        filters = [FieldFilter('uid', '==', uid), FieldFilter('capabilities', 'array_contains', 'persona')]
        persona_ref = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters))
        docs = persona_ref.get()
        if not docs:
            return []
        docs = [doc.to_dict() for doc in docs if 'omi' in doc.to_dict().get('connected_accounts', [])]
        return docs


    def get_omi_persona_apps_by_uid_db(uid: str):
        filters = [FieldFilter('uid', '==', uid), FieldFilter('category', '==', 'personality-emulation')]
        persona_ref = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters))
        docs = persona_ref.get()
        if not docs:
            return []
        docs = [doc.to_dict() for doc in docs]
        return docs


    def update_persona_in_db(persona_data: dict):
        persona_ref = db.collection(apps_collection).document(persona_data['id'])
        persona_ref.update(persona_data)


    def migrate_app_owner_id_db(new_id: str, old_id: str):
        filters = [FieldFilter('uid', '==', old_id)]
        apps_ref = db.collection(apps_collection).where(filter=BaseCompositeFilter('AND', filters)).stream()
        for app in apps_ref:
            app_ref = db.collection(apps_collection).document(app.id)
            app_ref.update({'uid': new_id})


    def create_api_key_db(app_id: str, api_key_data: dict):
        """Create a new API key for an app in the database"""
        api_key_ref = db.collection(apps_collection).document(app_id).collection('api_keys').document(api_key_data['id'])
        api_key_ref.set(api_key_data)
        return api_key_data


    def get_api_key_by_hash_db(app_id: str, hashed_key: str):
        """Get an API key by its hash value"""
        filters = [FieldFilter('hashed', '==', hashed_key)]
        api_keys_ref = (
            db.collection(apps_collection)
            .document(app_id)
            .collection('api_keys')
            .where(filter=BaseCompositeFilter('AND', filters))
            .limit(1)
        )
        docs = api_keys_ref.get()
        if not docs:
            return None
        doc = next(iter(docs), None)
        if not doc:
            return None
        return doc.to_dict()


    def delete_api_key_db(app_id: str, api_key_id: str):
        """Delete an API key from the database"""
        api_key_ref = db.collection(apps_collection).document(app_id).collection('api_keys').document(api_key_id)
        api_key_ref.delete()


    def get_api_keys_for_app_db(app_id: str) -> List[dict]:
        """Get all API keys for an app (without the hashed secret)"""
        api_keys_ref = db.collection(apps_collection).document(app_id).collection('api_keys').stream()
        return [doc.to_dict() for doc in api_keys_ref]


    def list_api_keys_db(app_id: str):
        """List all API keys for an app (excluding the hashed values)"""
        api_keys_ref = (
            db.collection(apps_collection)
            .document(app_id)
            .collection('api_keys')
            .order_by('created_at', direction='DESCENDING')
            .stream()
        )
        return [{k: v for k, v in doc.to_dict().items() if k != 'hashed'} for doc in api_keys_ref]


    def add_app_to_collection(app_id: str, collection_id: str):
        """Add an app to a collection"""
        collection_ref = db.collection('collections').document(collection_id)
        collection_ref.update({'app_ids': ArrayUnion([app_id])})


    def remove_app_from_collection(app_id: str, collection_id: str):
        """Remove an app from a collection"""
        collection_ref = db.collection('collections').document(collection_id)
        collection_ref.update({'app_ids': ArrayRemove([app_id])})


    def get_collection_db(collection_id: str):
        """Get a collection by ID"""
        collection_ref = db.collection('collections').document(collection_id)
        doc = collection_ref.get()
        if doc.exists:
            return doc.to_dict()
        return None


    def get_collections_for_user_db(uid: str) -> List[dict]:
        """Get all collections for a user"""
        collections_ref = db.collection('collections').where('uid', '==', uid).stream()
        return [doc.to_dict() for doc in collections_ref]


    def create_collection_db(collection_data: dict) -> dict:
        """Create a new collection"""
        collection_ref = db.collection('collections').document()
        collection_ref.set(collection_data)
        return collection_data


    def update_collection_in_db(collection_id: str, updates: dict):
        """Update a collection"""
        collection_ref = db.collection('collections').document(collection_id)
        collection_ref.update(updates)


    def delete_collection_from_db(collection_id: str):
        """Delete a collection"""
        collection_ref = db.collection('collections').document(collection_id)
        collection_ref.delete()


    def record_app_install(uid: str, app_id: str):
        """Record an app installation"""
        app_ref = db.collection(app_analytics_collection).document(app_id)
        app_ref.update({'installs': firestore.Increment(1)})
        installs_ref = app_ref.collection('installs').document(uid)
        installs_ref.set({'installed_at': datetime.now(timezone.utc), 'uid': uid})


    def increment_app_installs_count(app_id: str):
        """Increment the installs count for an app"""
        app_ref = db.collection(app_analytics_collection).document(app_id)
        app_ref.update({'total_installs': firestore.Increment(1)})


    def record_app_uninstall(uid: str, app_id: str):
        """Record an app uninstall"""
        installs_ref = db.collection(app_analytics_collection).document(app_id).collection('installs').document(uid)
        installs_ref.update({'uninstalled_at': datetime.now(timezone.utc)})


    def get_app_installs_count(app_id: str) -> int:
        """Get the total installs count for an app"""
        app_ref = db.collection(app_analytics_collection).document(app_id)
        doc = app_ref.get()
        if doc.exists:
            return doc.to_dict().get('total_installs', 0)
        return 0