export type AgentEvent =
  | { kind: 'text_chunk';         messageId: string; text: string }
  | { kind: 'thinking_chunk';     text: string }
  | { kind: 'tool_call';          toolCallId: string; title: string }
  | { kind: 'tool_output';        toolCallId: string; output: string; done: boolean }
  | { kind: 'permission_request'; requestId: string; toolCallId: string; options: PermissionOption[] }
  | { kind: 'turn_done';          stopReason: string; inputTokens?: number; outputTokens?: number; costUsd?: number }
  | { kind: 'session_id';         sessionId: string }

export interface PermissionOption {
  optionId: string
  name: string
  kind: string
}

export type PermissionDecision =
  | { type: 'selected'; optionId: string }
  | { type: 'cancelled' }
