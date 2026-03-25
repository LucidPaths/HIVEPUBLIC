import { useEffect, useRef } from 'react';
import * as api from '../lib/api';
import { buildTelegramPrompt, buildDiscordPrompt, buildAgentPrompt } from '../lib/channelPrompt';
import type { MessageOrigin } from '../types';

interface UseRemoteChannelsProps {
  sendMessageRef: React.MutableRefObject<((text?: string) => Promise<void>) | undefined>;
  messageOriginRef: React.MutableRefObject<MessageOrigin>;
  /** Called whenever a remote message is injected into chat (for tab notification). */
  onChatInjection?: () => void;
}

/**
 * Normalize a command path for matching: lowercase, strip .exe/.cmd/.bat,
 * extract just the binary name from a full path (handles / and \ separators).
 */
export function normalizeCommand(cmd: string): string {
  // Extract the last path component (handles both / and \ separators)
  const basename = cmd.split(/[/\\]/).pop() || cmd;
  // Strip common Windows executable extensions
  return basename.replace(/\.(exe|cmd|bat)$/i, '').toLowerCase();
}

/**
 * Try to route a message to a running terminal agent by command name.
 * Returns true if successfully routed, false if no matching session found.
 *
 * When routed to an agent, the message is written to the PTY's stdin with a
 * trailing newline (like pressing Enter). If no matching session exists,
 * returns false so the caller can fall back to chat routing (P4: graceful degradation).
 *
 * Command matching is normalized: "claude" matches "claude", "claude.exe",
 * "C:\Users\...\claude.exe", "/usr/bin/claude", etc.
 */
async function routeToAgent(agentCommand: string, text: string): Promise<boolean> {
  try {
    const sessions = await api.ptyList();
    const target = normalizeCommand(agentCommand);
    // Find a running, non-exited session whose command matches
    const match = sessions.find((s) =>
      !s.exited && normalizeCommand(s.command) === target
    );
    if (!match) return false;

    // Write message text + newline to the agent's stdin
    await api.ptyWrite(match.id, text + '\n');
    console.log(`[HIVE ROUTE] Routed to agent "${agentCommand}" (session ${match.id})`);
    return true;
  } catch (err) {
    console.warn(`[HIVE ROUTE] Failed to route to agent "${agentCommand}":`, err);
    return false;
  }
}

/**
 * Manages all remote channel event listeners:
 * - Telegram daemon (incoming messages → chat injection or terminal agent)
 * - Discord daemon (incoming messages → chat injection or terminal agent)
 * - Worker completion/messages/status updates
 * - Routines daemon (cron evaluator + triggered actions)
 *
 * Uses sendMessageRef to inject into whichever chat is active,
 * avoiding stale closure issues.
 *
 * Phase 10.5.4: Respects channel routing config — messages can be routed
 * to a running terminal agent instead of the chat pane.
 */
export function useRemoteChannels({ sendMessageRef, messageOriginRef, onChatInjection }: UseRemoteChannelsProps) {
  // Dead worker channel severance — track completed/failed/terminated worker IDs.
  // Messages from dead workers are discarded to prevent stale updates confusing the model.
  const completedWorkersRef = useRef<Set<string>>(new Set());

  // Stable ref for the injection callback — avoids stale closure in useEffect handlers.
  const onChatInjectionRef = useRef(onChatInjection);
  onChatInjectionRef.current = onChatInjection;

  // Telegram daemon — listen for incoming messages and auto-respond.
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    api.onTelegramMessage(async (msg) => {
      console.log(`[HIVE TELEGRAM] Message from ${msg.from_name} (@${msg.from_username}) [${msg.sender_role}]: ${msg.text.substring(0, 80)}`);

      // Set message origin BEFORE sending — tool approval checks this.
      messageOriginRef.current = msg.sender_role === 'host' ? 'remote-host' : 'remote-user';

      // Build prompt via shared channelPrompt module (P5: format + parser live together).
      const roleTag = msg.sender_role === 'host' ? 'Host' : 'User';
      const telegramPrompt = buildTelegramPrompt(msg.from_name, msg.from_username || undefined, msg.chat_id, roleTag, msg.wrapped_text);

      // Check routing config — route to terminal agent or chat pane
      const routing = api.getChannelRouting();
      if (routing.telegram !== 'chat') {
        const routed = await routeToAgent(routing.telegram, telegramPrompt);
        if (routed) return; // Successfully sent to agent, done
        // Fall through to chat if no matching session
        console.warn(`[HIVE TELEGRAM] No running "${routing.telegram}" session, falling back to chat`);
      }

      sendMessageRef.current?.(telegramPrompt);
      onChatInjectionRef.current?.();
    }).then(u => { unlisten = u; });

    return () => { unlisten?.(); };
  }, []);

  // Discord daemon — listen for incoming messages and auto-respond.
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    api.onDiscordMessage(async (msg) => {
      console.log(`[HIVE DISCORD] Message from ${msg.author_name} [${msg.sender_role}] in channel ${msg.channel_id}: ${msg.text.substring(0, 80)}`);

      // Set message origin BEFORE sending — tool approval checks this.
      messageOriginRef.current = msg.sender_role === 'host' ? 'remote-host' : 'remote-user';

      // Build prompt via shared channelPrompt module (P5: format + parser live together).
      const roleTag = msg.sender_role === 'host' ? 'Host' : 'User';
      const discordPrompt = buildDiscordPrompt(msg.author_name, msg.channel_id, msg.guild_id || undefined, roleTag, msg.wrapped_text);

      // Check routing config — route to terminal agent or chat pane
      const routing = api.getChannelRouting();
      if (routing.discord !== 'chat') {
        const routed = await routeToAgent(routing.discord, discordPrompt);
        if (routed) return; // Successfully sent to agent, done
        // Fall through to chat if no matching session
        console.warn(`[HIVE DISCORD] No running "${routing.discord}" session, falling back to chat`);
      }

      sendMessageRef.current?.(discordPrompt);
      onChatInjectionRef.current?.();
    }).then(u => { unlisten = u; });

    return () => { unlisten?.(); };
  }, []);

  // Worker completion — workers emit events when they finish (success or failure).
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    api.onWorkerCompleted((event) => {
      // Mark worker as dead — subsequent messages from this ID are discarded
      completedWorkersRef.current.add(event.worker_id);

      const statusEmoji = event.status === 'completed' ? 'completed' : 'FAILED';
      const detail = event.status === 'completed'
        ? (event.summary || 'No summary')
        : (event.error || 'Unknown error');
      console.log(`[HIVE WORKER] ${event.worker_id} ${statusEmoji}: ${detail}`);

      const workerPrompt = `[Worker ${event.worker_id} ${statusEmoji} after ${event.turns_used} turns | scratchpad: ${event.scratchpad_id}]\n${detail}`;
      sendMessageRef.current?.(workerPrompt);
      onChatInjectionRef.current?.();
    }).then(u => { unlisten = u; });

    return () => { unlisten?.(); };
  }, []);

  // Worker mid-task messages — workers use report_to_parent to ping the parent chat.
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    api.onWorkerMessage((event) => {
      // Dead worker severance: discard messages from workers that already completed/failed
      if (completedWorkersRef.current.has(event.worker_id)) {
        console.log(`[HIVE WORKER] Discarding message from dead worker ${event.worker_id}`);
        return;
      }

      const tag = event.severity === 'error' ? 'ERROR'
        : event.severity === 'warning' ? 'WARNING'
        : event.severity === 'done' ? 'DONE'
        : 'UPDATE';
      console.log(`[HIVE WORKER] ${event.worker_id} [${tag}]: ${event.message.substring(0, 100)}`);

      const workerPrompt = `[Worker ${event.worker_id} ${tag} | scratchpad: ${event.scratchpad_id}]\n${event.message}`;
      sendMessageRef.current?.(workerPrompt);
      onChatInjectionRef.current?.();
    }).then(u => { unlisten = u; });

    return () => { unlisten?.(); };
  }, []);

  // Worker periodic status updates — live observability for running workers.
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    api.onWorkerStatusUpdate((event) => {
      const timePct = event.max_time_seconds > 0
        ? ((event.elapsed_seconds / event.max_time_seconds) * 100).toFixed(0)
        : '?';
      console.log(
        `[HIVE WORKER] ${event.worker_id} heartbeat: turn ${event.turns_used}/${event.max_turns} | ` +
        `${event.elapsed_seconds}s/${event.max_time_seconds}s (${timePct}%) | ` +
        `${event.tools_executed} tools`
      );
    }).then(u => { unlisten = u; });

    return () => { unlisten?.(); };
  }, []);

  // Routines daemon — start cron evaluator and listen for triggered routines.
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    // Auto-start the cron daemon (evaluates cron-triggered routines every 60s)
    api.routinesDaemonStart().then(msg => {
      console.log(`[HIVE ROUTINES] ${msg}`);
    }).catch(err => {
      // Non-fatal — routines just won't fire cron triggers until memory is initialized
      console.warn('[HIVE ROUTINES] Daemon start deferred:', err);
    });

    // Listen for routine-triggered events (from cron or channel event matching)
    api.onRoutineTriggered((event) => {
      console.log(`[HIVE ROUTINES] Triggered: ${event.routine_name} — ${event.trigger_reason}`);

      // Set origin BEFORE calling sendMessage (P6: routines must NOT inherit stale origin).
      // Channel-triggered routines inherit the sender's role from the source event.
      // Cron-triggered routines are desktop-initiated (no remote user involved).
      if (event.source_event) {
        const senderRole = event.source_event.metadata?.sender_role as string | undefined;
        messageOriginRef.current =
          senderRole === 'host' ? 'remote-host' :
          senderRole === 'user' ? 'remote-user' :
          'desktop';
      } else {
        messageOriginRef.current = 'desktop';
      }

      // Build the prompt with routing info for the model
      let prompt = event.action_prompt;
      if (event.response_channel) {
        // Tell the model where to send the response
        const [channelType, channelId] = event.response_channel.split(':');
        if (channelType === 'telegram') {
          prompt += `\nRoute your response via telegram_send, chat_id "${channelId}".`;
        } else if (channelType === 'discord') {
          prompt += `\nRoute your response via discord_send, channel_id "${channelId}".`;
        }
      }

      sendMessageRef.current?.(prompt);
      onChatInjectionRef.current?.();

      // Record the run (we'll mark success optimistically — the agentic loop handles failures)
      api.routineRecordRun(event.routine_id, true, `Triggered: ${event.trigger_reason}`).catch(err => {
        console.warn('[HIVE ROUTINES] Failed to record run:', err);
      });
    }).then(u => { unlisten = u; });

    return () => {
      unlisten?.();
      api.routinesDaemonStop().catch(() => {});
    };
  }, []);

  // Agent bridge — listen for PTY agent responses (silence-detected output batches).
  // When a bridged terminal agent (Claude Code, Codex, etc.) finishes producing output,
  // the bridge monitor emits "agent-response" and we inject it into the active chat.
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    api.onAgentResponse((event) => {
      console.log(`[HIVE AGENT] Response from ${event.agent_name} (session ${event.session_id}): ${event.content.substring(0, 100)}...`);

      // PTY agent output gets desktop-level trust — it's running locally on the user's machine
      messageOriginRef.current = 'desktop';

      const agentPrompt = buildAgentPrompt(event.agent_name, event.session_id, event.content);
      sendMessageRef.current?.(agentPrompt);
      onChatInjectionRef.current?.();
    }).then(u => { unlisten = u; });

    return () => { unlisten?.(); };
  }, []);
}
