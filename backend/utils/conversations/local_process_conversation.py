"""
Local conversation processing for LOCAL_MODE.

Provides in-process conversation processing using the local LLM endpoint.
This replaces the pusher service when LOCAL_MODE=true.

Key features:
- Run in-process (no network call to pusher service)
- Call local LLM for summary/title/category extraction
- Extract action items and memories
- Write results to local SQLite

No cloud services (Firebase, Firestore, pusher) are used in LOCAL_MODE.
"""

import asyncio
import json
import logging
import os
import random
import re
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple, Union

from utils.llm.clients import get_local_llm, is_local_llm_enabled
from models.structured import ActionItem, Structured
from models.conversation import Conversation
from database.local_db import (
    upsert_conversation,
    get_conversation as local_get_conversation,
    save_memories,
    get_memory_ids_for_conversation,
    delete_memory,
    delete_memories_for_conversation,
    create_action_items_batch,
    delete_action_items_for_conversation,
    get_conversations as local_get_conversations,
)
from database.local_fts import upsert_conversation_fts, upsert_memory_fts, search_memories_fts

logger = logging.getLogger(__name__)

# LOCAL_MODE check
_LOCAL_MODE = os.getenv("LOCAL_MODE", "").lower() in ("1", "true", "yes")


def is_local_processing_available() -> bool:
    """True when local conversation processing is available."""
    return _LOCAL_MODE and is_local_llm_enabled()


async def _call_local_llm(prompt: str, system: str = "", feature: str = "conv_structure") -> str:
    """
    Call the local LLM endpoint with a prompt.
    
    Args:
        prompt: The user prompt to send to the LLM
        system: Optional system prompt
        feature: The feature/model to use
        
    Returns:
        The LLM response text
    """
    if not is_local_llm_enabled():
        raise RuntimeError("Local LLM not enabled")
    
    llm = get_local_llm(feature)
    if llm is None:
        raise RuntimeError("Failed to get local LLM client")
    
    from langchain_core.messages import HumanMessage, SystemMessage
    messages = []
    if system:
        messages.append(SystemMessage(content=system))
    messages.append(HumanMessage(content=prompt))
    
    response = await llm.ainvoke(messages)
    return response.content


def _build_conversation_text(conversation_data: Dict[str, Any]) -> str:
    """Build text representation of a conversation from its data."""
    text_parts = []
    
    if conversation_data.get("title"):
        text_parts.append(f"Title: {conversation_data['title']}")
    
    if conversation_data.get("overview"):
        text_parts.append(f"Overview: {conversation_data['overview']}")
    
    transcript = conversation_data.get("transcript_segments", [])
    if isinstance(transcript, list):
        for seg in transcript:
            speaker = seg.get("speaker", "Speaker")
            text = seg.get("text", "")
            if text:
                text_parts.append(f"{speaker}: {text}")
    
    return "\n".join(text_parts)


def _build_simple_context(conversation_data: Dict[str, Any]) -> str:
    """Build a simple context string from conversation data for LLM processing."""
    parts = []
    
    title = conversation_data.get("title", "")
    if title:
        parts.append(f"Title: {title}")
    
    transcript = conversation_data.get("transcript_segments", [])
    if isinstance(transcript, list):
        texts = []
        for seg in transcript:
            text = seg.get("text", "")
            if text:
                texts.append(text)
        if texts:
            parts.append("Transcript: " + " ".join(texts))
    
    return "\n\n".join(parts)


# ── Summary/Structure Extraction ───────────────────────────────────────────────

async def extract_conversation_structure(
    conversation_data: Dict[str, Any],
    language: str = "en",
) -> Dict[str, Any]:
    """
    Extract structured summary (title, overview, category, emoji) from conversation.
    
    Uses local LLM to generate:
    - title: Short conversation title (max 5 words)
    - overview: Brief summary (1-2 sentences)
    - category: One of work, personal, health, social, travel, shopping, other
    - emoji: Appropriate emoji
    """
    if not is_local_llm_enabled():
        return _fallback_structure(conversation_data)
    
    context = _build_simple_context(conversation_data)
    if not context.strip():
        return _fallback_structure(conversation_data)
    
    system_prompt = f"""You are a conversation summarization assistant. Analyze the conversation and extract key information.

Respond with ONLY a JSON object in this exact format (no markdown, no code blocks):
{{"title": "short title (max 5 words)", "overview": "brief summary (1-2 sentences)", "category": "work|personal|health|social|travel|shopping|other", "emoji": "single emoji"}}

Categories:
- work: job-related conversations, meetings, projects
- personal: personal matters, family, home
- health: health, fitness, medical topics
- social: catching up, casual conversations
- travel: trips, vacations, directions
- shopping: purchases, products, transactions
- other: anything that doesn't fit above

Be concise and extract the most important topics."""

    try:
        response_text = await _call_local_llm(
            f"Conversation:\n{context}",
            system=system_prompt,
            feature="conv_structure",
        )
        
        # Parse JSON response
        response_text = response_text.strip()
        # Remove markdown code blocks if present
        if response_text.startswith("```"):
            response_text = response_text.split("```")[1]
            if response_text.startswith("json"):
                response_text = response_text[4:]
        response_text = response_text.strip()
        
        result = json.loads(response_text)
        return {
            "title": result.get("title", "Conversation"),
            "overview": result.get("overview", ""),
            "category": result.get("category", "other"),
            "emoji": result.get("emoji", "💬"),
        }
    except Exception as e:
        logger.warning(f"Local LLM structure extraction failed: {e}")
        return _fallback_structure(conversation_data)


def _fallback_structure(conversation_data: Dict[str, Any]) -> Dict[str, Any]:
    """Generate a fallback structure when LLM is unavailable."""
    transcript = conversation_data.get("transcript_segments", [])
    if isinstance(transcript, list) and transcript:
        # Use first few words of first segment as title
        first_text = ""
        for seg in transcript:
            text = seg.get("text", "")
            if text:
                first_text = text
                break
        
        words = first_text.split()[:5]
        title = " ".join(words) if words else "Conversation"
    else:
        title = "Conversation"
    
    return {
        "title": title,
        "overview": "",
        "category": "other",
        "emoji": random.choice(["💬", "🗣️", "🎤"]),
    }


# ── Action Items Extraction ────────────────────────────────────────────────────

async def extract_action_items_local(
    conversation_data: Dict[str, Any],
    language: str = "en",
) -> List[Dict[str, Any]]:
    """
    Extract action items from conversation using local LLM.
    
    Returns list of action item dicts with:
    - description: The task description
    - due_at: Optional due date (ISO format)
    - completed: False
    """
    if not is_local_llm_enabled():
        return []
    
    context = _build_simple_context(conversation_data)
    if not context.strip():
        return []
    
    system_prompt = """You are an expert action item extractor. Your task is to identify actionable tasks from conversations.

Look for:
- Explicit requests: "remind me to X", "I need to X", "don't forget X"
- Commitments: "I'll X", "I promise to X", "I'll get back to you on X"
- Tasks mentioned: things that need to be done, follow-ups needed

Respond with ONLY a JSON array of action items in this format (no markdown, no code blocks):
[{"description": "task description", "due_at": null or "YYYY-MM-DD"}]

Only extract tasks that are:
1. Specific and actionable
2. Not already being done
3. Have a clear owner

If no action items found, return an empty array: []"""

    try:
        response_text = await _call_local_llm(
            f"Conversation:\n{context}",
            system=system_prompt,
            feature="conv_action_items",
        )
        
        # Parse JSON response
        response_text = response_text.strip()
        if response_text.startswith("```"):
            response_text = response_text.split("```")[1]
            if response_text.startswith("json"):
                response_text = response_text[4:]
        response_text = response_text.strip()
        
        items = json.loads(response_text)
        if not isinstance(items, list):
            return []
        
        now = datetime.now(timezone.utc)
        result = []
        for item in items:
            result.append({
                "description": item.get("description", ""),
                "due_at": item.get("due_at"),
                "completed": False,
                "created_at": now,
                "updated_at": now,
            })
        
        return result
    except Exception as e:
        logger.warning(f"Local LLM action items extraction failed: {e}")
        return []


# ── Memory Extraction ─────────────────────────────────────────────────────────

async def extract_memories_local(
    conversation_data: Dict[str, Any],
    language: str = "en",
) -> List[Dict[str, Any]]:
    """
    Extract key facts/memories from conversation using local LLM.
    
    Returns list of memory dicts with:
    - content: The fact or memory
    - category: One of fact, preference, plan, person, place, other
    """
    if not is_local_llm_enabled():
        return []
    
    context = _build_simple_context(conversation_data)
    if not context.strip():
        return []
    
    system_prompt = """You are a memory extraction assistant. Your task is to identify key facts and information from conversations that are worth remembering.

Extract:
- Personal facts: names, preferences, characteristics
- Plans and decisions: things agreed upon, future plans
- Important events: what happened, key outcomes
- Relationships: mentions of people and how they relate

Respond with ONLY a JSON array of memories in this format (no markdown, no code blocks):
[{"content": "memory description", "category": "fact|preference|plan|person|place|other"}]

Only extract information that is:
1. Specific and memorable
2. Likely to be useful in the future
3. Not trivial or obvious

If no meaningful memories found, return an empty array: []"""

    try:
        response_text = await _call_local_llm(
            f"Conversation:\n{context}",
            system=system_prompt,
            feature="memories",
        )
        
        # Parse JSON response
        response_text = response_text.strip()
        if response_text.startswith("```"):
            response_text = response_text.split("```")[1]
            if response_text.startswith("json"):
                response_text = response_text[4:]
        response_text = response_text.strip()
        
        items = json.loads(response_text)
        if not isinstance(items, list):
            return []
        
        now = datetime.now(timezone.utc)
        result = []
        for item in items:
            memory_id = str(uuid.uuid4())
            result.append({
                "id": memory_id,
                "content": item.get("content", ""),
                "category": item.get("category", "other"),
                "created_at": now,
                "updated_at": now,
            })
        
        return result
    except Exception as e:
        logger.warning(f"Local LLM memory extraction failed: {e}")
        return []


# ── Main Processing ───────────────────────────────────────────────────────────

async def process_conversation_locally(
    uid: str,
    conversation_id: str,
    language: str = "en",
) -> Optional[Dict[str, Any]]:
    """
    Process a conversation locally using local LLM.
    
    This is the main entry point for local conversation processing.
    It:
    1. Loads the conversation from local storage
    2. Extracts structured summary (title, overview, category, emoji)
    3. Extracts action items
    4. Extracts memories
    5. Saves results to local SQLite and FTS
    
    Args:
        uid: User ID
        conversation_id: Conversation ID to process
        language: Language code
        
    Returns:
        Updated conversation data or None if not found
    """
    if not is_local_processing_available():
        logger.warning("Local processing not available")
        return None
    
    # Load conversation from local storage
    conversation_data = local_get_conversation(uid, conversation_id)
    if not conversation_data:
        logger.warning(f"Conversation not found: {conversation_id}")
        return None
    
    logger.info(f"Processing conversation locally: {conversation_id}")
    
    # Extract structured summary
    structure = await extract_conversation_structure(conversation_data, language)
    
    # Update conversation with structure
    conversation_data["title"] = structure["title"]
    conversation_data["overview"] = structure.get("overview", "")
    if "structured" not in conversation_data or not conversation_data["structured"]:
        conversation_data["structured"] = {}
    conversation_data["structured"]["title"] = structure["title"]
    conversation_data["structured"]["overview"] = structure.get("overview", "")
    conversation_data["structured"]["category"] = structure.get("category", "other")
    conversation_data["structured"]["emoji"] = structure.get("emoji", "💬")
    conversation_data["structured"]["action_items"] = []
    
    # Extract and save action items
    action_items = await extract_action_items_local(conversation_data, language)
    if action_items:
        for item in action_items:
            item["conversation_id"] = conversation_id
        item_ids = create_action_items_batch(uid, action_items)
        logger.info(f"Saved {len(item_ids)} action items for {conversation_id}")
        conversation_data["structured"]["action_items"] = action_items
    
    # Extract and save memories
    memories = await extract_memories_local(conversation_data, language)
    if memories:
        for memory in memories:
            memory["conversation_id"] = conversation_id
        memory_ids = save_memories(uid, memories)
        logger.info(f"Saved {len(memory_ids)} memories for {conversation_id}")
        
        # Index memories in FTS
        for memory in memories:
            upsert_memory_fts(uid, memory["id"], memory["content"], memory.get("category", "other"))
    
    # Mark conversation as completed
    conversation_data["status"] = "completed"
    conversation_data["finished_at"] = datetime.now(timezone.utc).isoformat()
    
    # Save updated conversation
    upsert_conversation(uid, conversation_data)
    
    # Index conversation in FTS
    transcript_text = " ".join([
        seg.get("text", "")
        for seg in conversation_data.get("transcript_segments", [])
        if seg.get("text")
    ])
    upsert_conversation_fts(
        uid, 
        conversation_id, 
        structure.get("title", ""),
        transcript_text,
    )
    
    logger.info(f"Completed local processing: {conversation_id}")
    return conversation_data


def process_conversation_locally_sync(
    uid: str,
    conversation_id: str,
    language: str = "en",
) -> Optional[Dict[str, Any]]:
    """
    Synchronous wrapper for process_conversation_locally.
    
    Use this when calling from non-async context.
    """
    try:
        loop = asyncio.get_event_loop()
    except RuntimeError:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
    
    return loop.run_until_complete(
        process_conversation_locally(uid, conversation_id, language)
    )
