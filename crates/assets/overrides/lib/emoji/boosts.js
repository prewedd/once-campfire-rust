// Reactions go out as plain requests: the room's stream brings the new pill to everyone, you
// included, so no frame reloads (and none can race the stream).
export async function addBoost(url, content) {
  const body = new FormData()
  body.append("boost[content]", content)
  const response = await fetch(url, { method: "POST", body, redirect: "manual", headers: { Accept: "text/html" } })
  return response.ok || response.type === "opaqueredirect"
}
