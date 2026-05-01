from datetime import datetime, timezone
from typing import List, Optional, Dict, Any
import uuid
import os

# LOCAL_MODE: SQLite-only, no Firestore
_LOCAL_MODE = os.getenv("LOCAL_MODE", "").lower() in ("1", "true", "yes")

if _LOCAL_MODE:
    users_collection = "users"
    knowledge_nodes_collection = "knowledge_nodes"
    knowledge_edges_collection = "knowledge_edges"

    class KnowledgeNode:
        pass

    class KnowledgeEdge:
        pass

    def create_knowledge_node(uid: str, node_data: Dict[str, Any]):
        raise NotImplementedError("cloud-only")

    def get_knowledge_node(uid: str, node_id: str):
        return None

    def get_knowledge_nodes(uid: str, limit: int = 100, offset: int = 0):
        return []

    def update_knowledge_node(uid: str, node_id: str, updates: Dict[str, Any]):
        raise NotImplementedError("cloud-only")

    def delete_knowledge_node(uid: str, node_id: str):
        raise NotImplementedError("cloud-only")

    def create_knowledge_edge(uid: str, edge_data: Dict[str, Any]):
        raise NotImplementedError("cloud-only")

    def get_knowledge_edge(uid: str, edge_id: str):
        return None

    def get_knowledge_edges(uid: str, limit: int = 100, offset: int = 0):
        return []

    def update_knowledge_edge(uid: str, edge_id: str, updates: Dict[str, Any]):
        raise NotImplementedError("cloud-only")

    def delete_knowledge_edge(uid: str, edge_id: str):
        raise NotImplementedError("cloud-only")

    def get_knowledge_graph(uid: str):
        return {"nodes": [], "edges": []}

    def search_knowledge_nodes(uid: str, query: str, limit: int = 10):
        return []

    def delete_all_knowledge_data(uid: str):
        raise NotImplementedError("cloud-only")

else:
    from google.cloud import firestore
    from google.cloud.firestore_v1 import FieldFilter
    from ._client import db

    users_collection = "users"
    knowledge_nodes_collection = "knowledge_nodes"
    knowledge_edges_collection = "knowledge_edges"

    class KnowledgeNode:
        def __init__(self, id: str, name: str, node_type: str, properties: Dict[str, Any] = None):
            self.id = id
            self.name = name
            self.node_type = node_type
            self.properties = properties or {}

        def to_dict(self) -> Dict[str, Any]:
            return {
                "id": self.id,
                "name": self.name,
                "type": self.node_type,
                "properties": self.properties,
                "created_at": datetime.now(timezone.utc),
            }

    class KnowledgeEdge:
        def __init__(self, id: str, source_id: str, target_id: str, edge_type: str, properties: Dict[str, Any] = None):
            self.id = id
            self.source_id = source_id
            self.target_id = target_id
            self.edge_type = edge_type
            self.properties = properties or {}

        def to_dict(self) -> Dict[str, Any]:
            return {
                "id": self.id,
                "source_id": self.source_id,
                "target_id": self.target_id,
                "type": self.edge_type,
                "properties": self.properties,
                "created_at": datetime.now(timezone.utc),
            }

    def create_knowledge_node(uid: str, node_data: Dict[str, Any]):
        nodes_ref = db.collection("users").document(uid).collection(knowledge_nodes_collection)
        node_id = node_data.get("id", str(uuid.uuid4()))
        nodes_ref.document(node_id).set(node_data)
        return node_id

    def get_knowledge_node(uid: str, node_id: str):
        node_ref = db.collection("users").document(uid).collection(knowledge_nodes_collection).document(node_id)
        doc = node_ref.get()
        if doc.exists:
            return doc.to_dict()
        return None

    def get_knowledge_nodes(uid: str, limit: int = 100, offset: int = 0):
        nodes_ref = db.collection("users").document(uid).collection(knowledge_nodes_collection)
        return [doc.to_dict() for doc in nodes_ref.limit(limit).offset(offset).stream()]

    def update_knowledge_node(uid: str, node_id: str, updates: Dict[str, Any]):
        node_ref = db.collection("users").document(uid).collection(knowledge_nodes_collection).document(node_id)
        node_ref.update(updates)

    def delete_knowledge_node(uid: str, node_id: str):
        node_ref = db.collection("users").document(uid).collection(knowledge_nodes_collection).document(node_id)
        node_ref.delete()

    def create_knowledge_edge(uid: str, edge_data: Dict[str, Any]):
        edges_ref = db.collection("users").document(uid).collection(knowledge_edges_collection)
        edge_id = edge_data.get("id", str(uuid.uuid4()))
        edges_ref.document(edge_id).set(edge_data)
        return edge_id

    def get_knowledge_edge(uid: str, edge_id: str):
        edge_ref = db.collection("users").document(uid).collection(knowledge_edges_collection).document(edge_id)
        doc = edge_ref.get()
        if doc.exists:
            return doc.to_dict()
        return None

    def get_knowledge_edges(uid: str, limit: int = 100, offset: int = 0):
        edges_ref = db.collection("users").document(uid).collection(knowledge_edges_collection)
        return [doc.to_dict() for doc in edges_ref.limit(limit).offset(offset).stream()]

    def update_knowledge_edge(uid: str, edge_id: str, updates: Dict[str, Any]):
        edge_ref = db.collection("users").document(uid).collection(knowledge_edges_collection).document(edge_id)
        edge_ref.update(updates)

    def delete_knowledge_edge(uid: str, edge_id: str):
        edge_ref = db.collection("users").document(uid).collection(knowledge_edges_collection).document(edge_id)
        edge_ref.delete()

    def get_knowledge_graph(uid: str):
        nodes = get_knowledge_nodes(uid)
        edges = get_knowledge_edges(uid)
        return {"nodes": nodes, "edges": edges}

    def search_knowledge_nodes(uid: str, query: str, limit: int = 10):
        nodes_ref = db.collection("users").document(uid).collection(knowledge_nodes_collection)
        results = nodes_ref.where("name", ">=", query).where("name", "<=", query + "\uf8ff").limit(limit).stream()
        return [doc.to_dict() for doc in results]

    def delete_all_knowledge_data(uid: str):
        user_ref = db.collection("users").document(uid)
        for collection_name in [knowledge_nodes_collection, knowledge_edges_collection]:
            collection_ref = user_ref.collection(collection_name)
            for doc in collection_ref.stream():
                doc.reference.delete()
