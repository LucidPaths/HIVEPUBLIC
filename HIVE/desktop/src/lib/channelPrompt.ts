// channelPrompt.ts — Single source of truth for channel prompt format (P5: Fix The Pattern)
//
// The channel prompt header is the ONLY interface between:
//   - useRemoteChannels.ts (PRODUCER: builds the prompt when Telegram/Discord messages arrive)
//   - useChat.ts (CONSUMER: parses the prompt to detect external channel + route replies)
//
// If you change the format, the parser updates automatically because they live in the same file.
// This eliminates the class of bugs where format and regex drift apart.

export type ChannelType = 'telegram' | 'discord' | 'pty-agent';

export interface ChannelRoute {
  channel: ChannelType;
  chatId: string;
  senderName: string;
}

// ============================================
// FORMAT (producer)
// ============================================

/** Build a Telegram channel prompt header.
 *  Format: [Telegram from Name (@user) | chat: 12345 | Host] */
export function buildTelegramPrompt(
  senderName: string,
  username: string | undefined,
  chatId: string,
  role: 'Host' | 'User',
  wrappedText: string,
): string {
  const sender = username ? `${senderName} (@${username})` : senderName;
  return `[Telegram from ${sender} | chat: ${chatId} | ${role}]\n${wrappedText}`;
}

/** Build an agent response prompt header.
 *  Format: [Agent: claude | session: abc-123]
 *  Used by the bridge monitor to inject PTY agent output into orchestrator chat. */
export function buildAgentPrompt(
  agentName: string,
  sessionId: string,
  content: string,
): string {
  return `[Agent: ${agentName} | session: ${sessionId}]\n${content}`;
}

/** Build a Discord channel prompt header.
 *  Format: [Discord from Name | ch: 12345 | guild: 67890 | Host] */
export function buildDiscordPrompt(
  authorName: string,
  channelId: string,
  guildId: string | undefined,
  role: 'Host' | 'User',
  wrappedText: string,
): string {
  const guild = guildId ? ` | guild: ${guildId}` : '';
  return `[Discord from ${authorName} | ch: ${channelId}${guild} | ${role}]\n${wrappedText}`;
}

// ============================================
// PARSE (consumer)
// ============================================

// Regexes match the header format above.
// The (?:\s*\|[^\]]*?)? group handles optional trailing pipe-separated fields
// (role tags, guild IDs, future metadata) without breaking when new fields are added.
const TG_REGEX = /^\[Telegram from (.+?)\s*\| chat: (\S+?)(?:\s*\|[^\]]*?)?\]/;
const DC_REGEX = /^\[Discord from (.+?)\s*\| ch: (\S+?)(?:\s*\|[^\]]*?)?\]/;
const AGENT_REGEX = /^\[Agent: (.+?)\s*\| session: (\S+?)(?:\s*\|[^\]]*?)?\]/;

/** Parse a channel prompt header. Returns null if the message is a regular HIVE chat message. */
export function parseChannelPrompt(content: string): ChannelRoute | null {
  const tgMatch = content.match(TG_REGEX);
  if (tgMatch) {
    return {
      channel: 'telegram',
      chatId: tgMatch[2],
      senderName: tgMatch[1].replace(/\s*\(@\w+\)$/, ''),
    };
  }

  const dcMatch = content.match(DC_REGEX);
  if (dcMatch) {
    return {
      channel: 'discord',
      chatId: dcMatch[2],
      senderName: dcMatch[1].replace(/\s*\(@\w+\)$/, ''),
    };
  }

  const agentMatch = content.match(AGENT_REGEX);
  if (agentMatch) {
    return {
      channel: 'pty-agent',
      chatId: agentMatch[2], // session_id
      senderName: agentMatch[1], // agent name (e.g., "claude")
    };
  }

  return null;
}
