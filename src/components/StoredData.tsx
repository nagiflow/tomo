import { useEffect, useState } from "react";
import { Search } from "lucide-react";
import type { UnifiedMessage } from "../types";
import { providerLabels } from "../types";
import { command } from "../services/api";
import { time } from "./ui";
export function StoredData() {
  const [messages, setMessages] = useState<UnifiedMessage[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    void command<UnifiedMessage[]>("get_cached_messages")
      .then(setMessages)
      .catch((e) => setError(String(e)));
  }, []);
  const filtered = messages.filter((m) =>
    `${m.sender.name} ${m.sender.address} ${m.content} ${m.metadata.subject || ""}`
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  return (
    <div className="stored-data">
      <label className="search">
        <Search size={15} />
        <input
          aria-label="Search cached messages"
          placeholder="Search cached messages"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>
      <p className="muted">
        {filtered.length} of {messages.length} cached messages. Search runs on
        this device.
      </p>
      {error && <p role="alert">{error}</p>}
      <div className="cached-messages">
        {filtered.map((m) => (
          <details key={m.id}>
            <summary>
              <strong>{m.sender.name}</strong> · {providerLabels[m.provider]} ·{" "}
              {time(m.timestamp)}
            </summary>
            <h3>{String(m.metadata.subject || "Conversation")}</h3>
            <p>{m.content}</p>
          </details>
        ))}
        {!filtered.length && (
          <p className="muted">No cached messages match this search.</p>
        )}
      </div>
    </div>
  );
}
