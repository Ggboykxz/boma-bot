#!/usr/bin/env python3
"""A deterministic ACP stub for tests (REQ-061). Wire replay only; never a real agent.

Speaks newline-delimited JSON-RPC 2.0 on stdin/stdout. Answers initialize, session/new,
session/prompt (two agent_message_chunk notifications then end_turn), session/close.
"""
import json
import sys

NEXT_ID = 1000
ASK_PERMISSION = "--ask-permission" in sys.argv

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

def main():
    global NEXT_ID
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        method = msg.get("method")
        msg_id = msg.get("id")
        if method == "initialize":
            send({
                "jsonrpc": "2.0",
                "id": msg_id,
                "result": {
                    "protocolVersion": 1,
                    "agentCapabilities": {},
                    "agentInfo": {"name": "stub-acp-agent", "title": "Stub", "version": "0.0.0"},
                    "authMethods": [],
                },
            })
        elif method == "session/new":
            send({
                "jsonrpc": "2.0",
                "id": msg_id,
                "result": {"sessionId": "stub_session_1"},
            })
        elif method == "session/prompt":
            if ASK_PERMISSION:
                # Ask the client for permission before answering, wait for its decision,
                # and echo it so tests can assert which outcome landed.
                perm_id = NEXT_ID
                NEXT_ID += 1
                send({
                    "jsonrpc": "2.0",
                    "id": perm_id,
                    "method": "session/request_permission",
                    "params": {
                        "sessionId": msg["params"]["sessionId"],
                        "toolCall": {
                            "toolCallId": "tc_1",
                            "title": "edit file",
                            "kind": "edit",
                            "status": "pending",
                        },
                        "options": [
                            {"optionId": "allow_once", "name": "Allow once", "kind": "allow_once"},
                            {"optionId": "reject_once", "name": "Reject once", "kind": "reject_once"},
                        ],
                    },
                })
                outcome = None
                while True:
                    line = sys.stdin.readline()
                    if not line:
                        break
                    reply = json.loads(line)
                    if reply.get("id") == perm_id:
                        outcome = reply.get("result")
                        break
                detail = json.dumps(outcome)
                chunks = ["permission:", detail, f" after:{msg['params']['prompt'][0]['text']}"]
            else:
                text = msg["params"]["prompt"][0]["text"]
                chunks = ["stub says: ", text.upper()]
            for chunk in chunks:
                send({
                    "jsonrpc": "2.0",
                    "method": "session/update",
                    "params": {
                        "sessionId": msg["params"]["sessionId"],
                        "update": {
                            "sessionUpdate": "agent_message_chunk",
                            "messageId": "msg_stub",
                            "content": {"type": "text", "text": chunk},
                        },
                    },
                })
            send({
                "jsonrpc": "2.0",
                "id": msg_id,
                "result": {"stopReason": "end_turn"},
            })
        elif method == "session/close":
            send({
                "jsonrpc": "2.0",
                "id": msg_id,
                "result": {},
            })
        else:
            send({
                "jsonrpc": "2.0",
                "id": msg_id,
                "error": {"code": -32601, "message": "method not found"},
            })

if __name__ == "__main__":
    main()
