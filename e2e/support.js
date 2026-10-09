// Shared by the suites.

// A direct room for just these two people, both looking at it: suites that start huddles use one,
// so suites running in parallel don't join each other's (or ring each other's users).
exports.privateRoom = async (BASE, a, b) => {
  const id = await b.evaluate(() => Current.user.id)
  const url = await a.evaluate(async id => (await fetch(`/rooms/directs?user_ids[]=${id}`, { method: "POST", headers: { Accept: "text/html" } })).url, id)
  const room = url.match(/rooms\/(\d+)/)?.[1]
  if (!room) throw new Error(`no direct room: ${url}`)
  for (const p of [ a, b ]) {
    await p.goto(`${BASE}/rooms/${room}`)
    await p.waitForLoadState("networkidle")
  }
  return room
}
