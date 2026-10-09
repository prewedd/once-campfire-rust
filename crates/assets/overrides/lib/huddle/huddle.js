// Huddles (plans/huddles.md): a room's drop-in voice and video call, on LiveKit.
//
// One call at a time, kept across page visits: this module lives as long as the tab, and its UI
// lives in #huddle, which every page has and Turbo keeps (data-turbo-permanent). The bar along
// the bottom of the sidebar shows who's there and the controls; "expand" opens the stage, with
// everyone's video or avatar, screens being shared, and each person's volume.

import { cable } from "@hotwired/turbo-rails"

const LIVEKIT_URL = RAILS_ASSET_URL("/livekit/livekit-client.esm.js")
const PREFS_KEY = "huddle-prefs"
const VOLUMES_KEY = "huddle-volumes"
const REACTIONS = [ "👍", "❤️", "😂", "🎉", "👏", "🙌" ]

const ICONS = {
  mic: '<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v3"/>',
  micOff: '<path d="m2 2 20 20M18.9 13.4A7 7 0 0 0 19 12v-2M5 10v2a7 7 0 0 0 12 5M15 9.3V5a3 3 0 0 0-5.7-1.3M9 9v3a3 3 0 0 0 5.1 2.1M12 19v3"/>',
  camera: '<path d="m22 8-6 4 6 4V8Z"/><rect x="2" y="6" width="14" height="12" rx="2"/>',
  cameraOff: '<path d="m2 2 20 20M10.7 6H14a2 2 0 0 1 2 2v3.3l1 1L22 8v8M16 16a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h2"/>',
  screen: '<rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4M9 10l3-3 3 3M12 7v6"/>',
  smile: '<circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2M9 9h.01M15 9h.01"/>',
  hand: '<path d="M18 11V6a2 2 0 0 0-4 0v5M14 10V4a2 2 0 0 0-4 0v6M10 10.5V6a2 2 0 0 0-4 0v8"/><path d="M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.9-6-2.4l-3.6-3.6a2 2 0 0 1 2.8-2.8L7 15"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z"/>',
  expand: '<path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/>',
  collapse: '<path d="M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7"/>',
  leave: '<path d="M10.7 13.3a16 16 0 0 0 3.4 2.6l1.3-1.3a2 2 0 0 1 2.1-.5 13 13 0 0 0 2.8.7 2 2 0 0 1 1.7 2v3a2 2 0 0 1-2.2 2 19.8 19.8 0 0 1-8.6-3.1M5.2 13.4A19.8 19.8 0 0 1 2.1 4.2 2 2 0 0 1 4.1 2h3a2 2 0 0 1 2 1.7c.1.9.4 1.9.7 2.8a2 2 0 0 1-.5 2.1L8 9.9M22 2 2 22"/>',
  volume: '<path d="M11 5 6 9H2v6h4l5 4V5ZM15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14"/>',
}

function icon(name) {
  return `<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name]}</svg>`
}

function escapeHTML(text) {
  return String(text ?? "").replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char])
}

function load(key, fallback) {
  try {
    return { ...fallback, ...JSON.parse(localStorage.getItem(key)) }
  } catch {
    return fallback
  }
}

function save(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // Private windows: preferences last for this tab
  }
}

// Short tones for joining and leaving, made on the spot.
function chime(up) {
  try {
    const context = chime.context ??= new AudioContext()
    const now = context.currentTime
    ;[ 0, 0.09 ].forEach((offset, index) => {
      const oscillator = context.createOscillator()
      const gain = context.createGain()
      oscillator.frequency.value = (up ? [ 660, 880 ] : [ 660, 495 ])[index]
      gain.gain.setValueAtTime(0.0001, now + offset)
      gain.gain.exponentialRampToValueAtTime(0.08, now + offset + 0.02)
      gain.gain.exponentialRampToValueAtTime(0.0001, now + offset + 0.18)
      oscillator.connect(gain).connect(context.destination)
      oscillator.start(now + offset)
      oscillator.stop(now + offset + 0.2)
    })
  } catch {
    // No audio context: no chime
  }
}

class Huddle extends EventTarget {
  roomId = null
  roomName = ""
  #lk
  #room
  #startedAt
  #timer
  #prefs = load(PREFS_KEY, { noiseSuppression: true, echoCancellation: true, autoGainControl: true, audioinput: "", audiooutput: "", videoinput: "" })
  #volumes = load(VOLUMES_KEY, {})
  #reactions = new Map()
  #status = ""

  get connected() {
    return !!this.#room
  }

  // Joining and leaving

  async join(roomId, roomName) {
    if (this.roomId === roomId) return this.expand(true)
    if (this.connected) await this.leave()

    this.roomId = roomId
    this.roomName = roomName
    this.#status = "Connecting…"
    this.#render()

    try {
      const [ lk, { url, token } ] = await Promise.all([ this.#livekit(), this.#token(roomId) ])
      const room = new lk.Room({
        adaptiveStream: true,
        dynacast: true,
        webAudioMix: true,
        audioCaptureDefaults: this.#audioOptions(),
        videoCaptureDefaults: { resolution: lk.VideoPresets.h720.resolution, deviceId: this.#prefs.videoinput || undefined },
        audioOutput: this.#prefs.audiooutput ? { deviceId: this.#prefs.audiooutput } : undefined,
        publishDefaults: { simulcast: true, dtx: true, red: true },
      })
      this.#room = room
      this.#listen(room, lk)
      await room.connect(url, token)
      await room.startAudio()
      this.#startedAt = Date.now()
      this.#timer = setInterval(() => this.#renderTimer(), 1000)
      this.#status = ""
      chime(true)
      try {
        await room.localParticipant.setMicrophoneEnabled(true)
      } catch {
        this.#status = "Your microphone is blocked: allow it in the browser to talk"
      }
      this.#render()
    } catch (error) {
      console.error("huddle:", error)
      this.#status = ""
      await this.leave({ quietly: true })
      this.#flash("Couldn’t join the huddle. Check your connection and try again.")
    }
  }

  async leave({ quietly = false } = {}) {
    const room = this.#room
    this.#room = null
    clearInterval(this.#timer)
    if (room) {
      await room.disconnect()
      if (!quietly) chime(false)
    }
    this.roomId = null
    this.roomName = ""
    this.#status = ""
    this.#render()
  }

  async #livekit() {
    return this.#lk ??= await import(LIVEKIT_URL)
  }

  async #token(roomId) {
    const announce = !huddles.get(String(roomId))?.length
    const response = await fetch(`/rooms/${roomId}/huddle${announce ? "?announce=1" : ""}`, {
      method: "POST",
      headers: { Accept: "application/json", "X-CSRF-Token": document.querySelector('meta[name="csrf-token"]')?.content ?? "" },
    })
    if (!response.ok) throw new Error(`huddle token: ${response.status}`)
    return response.json()
  }

  // Controls

  async toggleMic() {
    const local = this.#room?.localParticipant
    if (!local) return
    try {
      await local.setMicrophoneEnabled(!local.isMicrophoneEnabled)
    } catch {
      this.#flash("Your microphone is blocked: allow it in the browser’s site settings")
    }
    this.#render()
  }

  async toggleCamera() {
    const local = this.#room?.localParticipant
    if (!local) return
    try {
      await local.setCameraEnabled(!local.isCameraEnabled)
      if (local.isCameraEnabled) this.expand(true)
    } catch {
      this.#flash("Your camera is blocked or busy")
    }
    this.#render()
  }

  async toggleScreen() {
    const local = this.#room?.localParticipant
    if (!local) return
    if (!navigator.mediaDevices?.getDisplayMedia) return this.#flash("This browser can’t share its screen")
    try {
      await local.setScreenShareEnabled(!local.isScreenShareEnabled, { audio: true, selfBrowserSurface: "exclude" })
    } catch {
      // Cancelled the browser's picker
    }
    this.#render()
  }

  async raiseHand() {
    const local = this.#room?.localParticipant
    if (!local) return
    const raised = local.attributes?.hand === "1"
    await local.setAttributes({ hand: raised ? "" : "1" })
    this.#render()
  }

  async react(emoji) {
    const room = this.#room
    if (!room) return
    await room.localParticipant.publishData(new TextEncoder().encode(emoji), { reliable: true, topic: "reaction" })
    this.#showReaction(room.localParticipant.identity, emoji)
  }

  expand(open = !this.#element.classList.contains("huddle--expanded")) {
    this.#element.classList.toggle("huddle--expanded", open)
    this.#render()
  }

  setVolume(identity, percent) {
    const participant = this.#room?.remoteParticipants.get(identity)
    this.#volumes[identity] = percent
    save(VOLUMES_KEY, this.#volumes)
    participant?.setVolume(percent / 100)
    participant?.setVolume(percent / 100, "screen_share_audio")
  }

  async setDevice(kind, deviceId) {
    this.#prefs[kind] = deviceId
    save(PREFS_KEY, this.#prefs)
    await this.#room?.switchActiveDevice(kind, deviceId)
  }

  async setProcessing(name, enabled) {
    this.#prefs[name] = enabled
    save(PREFS_KEY, this.#prefs)
    const track = this.#room?.localParticipant.getTrackPublication("microphone")?.track
    await track?.restartTrack(this.#audioOptions())
  }

  #audioOptions() {
    const { noiseSuppression, echoCancellation, autoGainControl, audioinput } = this.#prefs
    return { noiseSuppression, echoCancellation, autoGainControl, voiceIsolation: noiseSuppression, deviceId: audioinput || undefined }
  }

  // LiveKit's events

  #listen(room, lk) {
    const { RoomEvent } = lk
    const render = () => this.#render()
    room
      .on(RoomEvent.ParticipantConnected, participant => {
        participant.setVolume((this.#volumes[participant.identity] ?? 100) / 100)
        chime(true)
        render()
      })
      .on(RoomEvent.ParticipantDisconnected, () => { chime(false); render() })
      .on(RoomEvent.TrackSubscribed, (track, publication, participant) => {
        if (track.kind === "audio") {
          this.#element.querySelector(".huddle__audio").append(track.attach())
          participant.setVolume((this.#volumes[participant.identity] ?? 100) / 100, publication.source)
        }
        if (publication.source === "screen_share") this.expand(true)
        render()
      })
      .on(RoomEvent.TrackUnsubscribed, track => { track.detach().forEach(element => element.remove()); render() })
      .on(RoomEvent.TrackMuted, render)
      .on(RoomEvent.TrackUnmuted, render)
      .on(RoomEvent.LocalTrackPublished, render)
      .on(RoomEvent.LocalTrackUnpublished, render)
      .on(RoomEvent.ActiveSpeakersChanged, () => this.#renderSpeaking())
      .on(RoomEvent.ParticipantAttributesChanged, render)
      .on(RoomEvent.ConnectionQualityChanged, render)
      .on(RoomEvent.Reconnecting, () => { this.#status = "Reconnecting…"; render() })
      .on(RoomEvent.Reconnected, () => { this.#status = ""; render() })
      .on(RoomEvent.AudioPlaybackStatusChanged, () => {
        this.#status = room.canPlaybackAudio ? "" : "Click to hear the huddle"
        render()
      })
      .on(RoomEvent.DataReceived, (payload, participant, _kind, topic) => {
        if (topic === "reaction" && participant) this.#showReaction(participant.identity, new TextDecoder().decode(payload).slice(0, 8))
      })
      .on(RoomEvent.Disconnected, () => {
        if (this.#room === room) this.leave({ quietly: true })
      })
  }

  #showReaction(identity, emoji) {
    this.#reactions.set(identity, emoji)
    this.#render()
    setTimeout(() => {
      if (this.#reactions.get(identity) === emoji) {
        this.#reactions.delete(identity)
        this.#render()
      }
    }, 3000)
  }

  // Rendering

  get #element() {
    return document.getElementById("huddle")
  }

  #people() {
    const room = this.#room
    if (!room) return []
    return [ room.localParticipant, ...room.remoteParticipants.values() ].map(participant => {
      let metadata = {}
      try { metadata = JSON.parse(participant.metadata || "{}") } catch {}
      const screen = participant.getTrackPublication("screen_share")
      const camera = participant.getTrackPublication("camera")
      return {
        participant,
        identity: participant.identity,
        name: participant.name || participant.identity,
        avatar: metadata.avatar_url,
        local: participant === room.localParticipant,
        muted: !participant.isMicrophoneEnabled,
        speaking: participant.isSpeaking,
        hand: participant.attributes?.hand === "1",
        reaction: this.#reactions.get(participant.identity),
        quality: participant.connectionQuality,
        camera: camera?.track && !camera.isMuted ? camera : null,
        screen: screen?.track ? screen : null,
      }
    })
  }

  #render() {
    const element = this.#element
    if (!element) return
    element.hidden = !this.roomId
    if (!this.roomId) {
      element.classList.remove("huddle--expanded")
      element.replaceChildren()
      this.dispatchEvent(new Event("change"))
      return
    }
    if (!element.querySelector(".huddle__audio")) this.#build(element)

    const people = this.#people()
    const local = this.#room?.localParticipant
    const expanded = element.classList.contains("huddle--expanded")

    element.querySelector(".huddle__room").textContent = this.roomName
    element.querySelector(".huddle__status").textContent = this.#status
    element.querySelector(".huddle__status").hidden = !this.#status
    element.querySelector(".huddle__people").innerHTML = people.map(person => `
      <span class="huddle__person${person.speaking ? " huddle__person--speaking" : ""}" title="${escapeHTML(person.name)}${person.muted ? " (muted)" : ""}">
        ${person.avatar ? `<img src="${escapeHTML(person.avatar)}" alt="">` : `<span>${escapeHTML(person.name.slice(0, 1))}</span>`}
        ${person.muted ? `<i class="huddle__muted">${icon("micOff")}</i>` : ""}
        ${person.hand ? `<i class="huddle__hand">✋</i>` : ""}
        ${person.reaction ? `<i class="huddle__reaction">${escapeHTML(person.reaction)}</i>` : ""}
      </span>`).join("")

    this.#setControl("mic", local?.isMicrophoneEnabled, local?.isMicrophoneEnabled ? "Mute (⌘⇧Space)" : "Unmute (⌘⇧Space)", local?.isMicrophoneEnabled ? "mic" : "micOff")
    this.#setControl("camera", local?.isCameraEnabled, local?.isCameraEnabled ? "Turn off camera" : "Turn on camera", local?.isCameraEnabled ? "camera" : "cameraOff")
    this.#setControl("screen", local?.isScreenShareEnabled, local?.isScreenShareEnabled ? "Stop sharing" : "Share your screen", "screen")
    this.#setControl("hand", local?.attributes?.hand === "1", "Raise your hand", "hand")
    this.#setControl("expand", expanded, expanded ? "Collapse" : "Expand", expanded ? "collapse" : "expand")

    if (expanded) this.#renderStage(people)
    this.#renderTimer()
    this.dispatchEvent(new Event("change"))
  }

  #setControl(name, on, title, iconName) {
    this.#element.querySelectorAll(`[data-huddle-control="${name}"]`).forEach(button => {
      button.classList.toggle("huddle__control--on", !!on)
      button.setAttribute("aria-pressed", !!on)
      button.title = title
      button.setAttribute("aria-label", title)
      button.innerHTML = icon(iconName)
    })
  }

  #renderSpeaking() {
    const speaking = new Set(this.#people().filter(person => person.speaking).map(person => person.identity))
    this.#element?.querySelectorAll("[data-identity]").forEach(element => element.classList.toggle("huddle__tile--speaking", speaking.has(element.dataset.identity)))
    this.#element?.querySelectorAll(".huddle__person").forEach((element, index) => {
      element.classList.toggle("huddle__person--speaking", speaking.has(this.#people()[index]?.identity))
    })
  }

  #renderTimer() {
    const timer = this.#element?.querySelector(".huddle__timer")
    if (!timer || !this.#startedAt) return
    const seconds = Math.floor((Date.now() - this.#startedAt) / 1000)
    const parts = [ Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60 ]
    timer.textContent = (parts[0] ? parts : parts.slice(1)).map((part, index) => index ? String(part).padStart(2, "0") : part).join(":")
  }

  // The stage: a tile per screen being shared, then one per person.
  #renderStage(people) {
    const grid = this.#element.querySelector(".huddle__grid")
    const tiles = [
      ...people.filter(person => person.screen).map(person => ({ key: `${person.identity}:screen`, person, track: person.screen.track, screen: true })),
      ...people.map(person => ({ key: person.identity, person, track: person.camera?.track })),
    ]
    const keys = new Set(tiles.map(tile => tile.key))
    grid.querySelectorAll("[data-key]").forEach(element => keys.has(element.dataset.key) || element.remove())

    tiles.forEach((tile, index) => {
      let element = grid.querySelector(`[data-key="${CSS.escape(tile.key)}"]`)
      if (!element) {
        element = document.createElement("div")
        element.dataset.key = tile.key
        element.dataset.identity = tile.person.identity
        element.className = `huddle__tile${tile.screen ? " huddle__tile--screen" : ""}`
        element.innerHTML = `<div class="huddle__media"></div><div class="huddle__caption"></div>`
      }
      if (grid.children[index] !== element) grid.insertBefore(element, grid.children[index] ?? null)

      const media = element.querySelector(".huddle__media")
      const video = media.querySelector("video")
      if (tile.track && video?.dataset.sid !== tile.track.sid) {
        media.replaceChildren()
        const attached = tile.track.attach()
        attached.dataset.sid = tile.track.sid
        attached.muted = true
        media.append(attached)
      } else if (!tile.track && !media.querySelector(".huddle__avatar")) {
        media.innerHTML = tile.person.avatar
          ? `<img class="huddle__avatar" src="${escapeHTML(tile.person.avatar)}" alt="">`
          : `<span class="huddle__avatar huddle__avatar--initial">${escapeHTML(tile.person.name.slice(0, 1))}</span>`
      }
      element.classList.toggle("huddle__tile--mirrored", tile.person.local && !tile.screen)
      element.classList.toggle("huddle__tile--speaking", tile.person.speaking && !tile.screen)

      const volume = this.#volumes[tile.person.identity] ?? 100
      element.querySelector(".huddle__caption").innerHTML = `
        <span class="huddle__name">${tile.person.muted && !tile.screen ? icon("micOff") : ""}${escapeHTML(tile.person.name)}${tile.screen ? "’s screen" : ""}${tile.person.local ? " (you)" : ""}</span>
        ${tile.person.hand && !tile.screen ? `<span class="huddle__hand">✋</span>` : ""}
        ${tile.person.reaction && !tile.screen ? `<span class="huddle__tile-reaction">${escapeHTML(tile.person.reaction)}</span>` : ""}
        ${tile.person.quality === "poor" || tile.person.quality === "lost" ? `<span class="huddle__quality" title="Poor connection">●</span>` : ""}
        ${tile.person.local || tile.screen ? "" : `
          <label class="huddle__volume" title="Volume for you">${icon("volume")}
            <input type="range" min="0" max="200" step="5" value="${volume}" data-volume="${escapeHTML(tile.person.identity)}" aria-label="${escapeHTML(tile.person.name)}’s volume">
            <output>${volume}%</output>
          </label>`}`
    })
  }

  #build(element) {
    element.innerHTML = `
      <div class="huddle__window" role="region" aria-label="Huddle">
        <div class="huddle__head">
          <span class="huddle__live" aria-hidden="true"></span>
          <span class="huddle__room"></span>
          <span class="huddle__timer"></span>
          <button type="button" class="huddle__control huddle__control--plain" data-huddle-control="expand"></button>
        </div>
        <p class="huddle__status" hidden></p>
        <div class="huddle__grid"></div>
        <div class="huddle__people"></div>
        <div class="huddle__controls">
          <button type="button" class="huddle__control" data-huddle-control="mic"></button>
          <button type="button" class="huddle__control" data-huddle-control="camera"></button>
          <button type="button" class="huddle__control" data-huddle-control="screen"></button>
          <div class="huddle__menu">
            <button type="button" class="huddle__control" data-huddle-open="reactions" title="React" aria-label="React">${icon("smile")}</button>
            <div class="huddle__popover huddle__reactions" data-huddle-popover="reactions" hidden>
              ${REACTIONS.map(emoji => `<button type="button" data-huddle-react="${emoji}">${emoji}</button>`).join("")}
              <button type="button" class="huddle__control" data-huddle-control="hand"></button>
            </div>
          </div>
          <div class="huddle__menu">
            <button type="button" class="huddle__control" data-huddle-open="settings" title="Settings" aria-label="Settings">${icon("settings")}</button>
            <div class="huddle__popover huddle__settings" data-huddle-popover="settings" hidden></div>
          </div>
          <button type="button" class="huddle__control huddle__control--leave" data-huddle-leave title="Leave" aria-label="Leave">${icon("leave")}<span>Leave</span></button>
        </div>
      </div>
      <div class="huddle__audio" hidden></div>`

    element.addEventListener("click", this.#click)
    element.addEventListener("input", this.#input)
    element.addEventListener("change", this.#change)
  }

  #click = (event) => {
    const target = event.target.closest("button, [data-huddle-status]")
    if (this.#status && this.#room && !this.#room.canPlaybackAudio) this.#room.startAudio()
    if (!target) return
    const control = target.dataset.huddleControl
    if (control === "mic") this.toggleMic()
    else if (control === "camera") this.toggleCamera()
    else if (control === "screen") this.toggleScreen()
    else if (control === "hand") this.raiseHand()
    else if (control === "expand") this.expand()
    else if (target.dataset.huddleReact) this.react(target.dataset.huddleReact)
    else if (target.dataset.huddleLeave !== undefined) this.leave()
    else if (target.dataset.huddleOpen) this.#togglePopover(target.dataset.huddleOpen)
  }

  #input = (event) => {
    const slider = event.target.closest("[data-volume]")
    if (!slider) return
    this.setVolume(slider.dataset.volume, Number(slider.value))
    slider.nextElementSibling.textContent = `${slider.value}%`
  }

  #change = (event) => {
    const field = event.target
    if (field.dataset.device) this.setDevice(field.dataset.device, field.value)
    if (field.dataset.processing) this.setProcessing(field.dataset.processing, field.checked)
  }

  async #togglePopover(name) {
    const popover = this.#element.querySelector(`[data-huddle-popover="${name}"]`)
    const open = popover.hidden
    this.#element.querySelectorAll("[data-huddle-popover]").forEach(other => { other.hidden = true })
    popover.hidden = !open
    if (open && name === "settings") await this.#renderSettings(popover)
  }

  async #renderSettings(popover) {
    const lk = await this.#livekit()
    const kinds = [ [ "audioinput", "Microphone" ], [ "audiooutput", "Speaker" ], [ "videoinput", "Camera" ] ]
    const options = await Promise.all(kinds.map(([ kind ]) => lk.Room.getLocalDevices(kind, false).catch(() => [])))
    const active = kind => this.#room?.getActiveDevice(kind) || this.#prefs[kind]
    const outputSelectable = lk.supportsAudioOutputSelection?.() ?? true
    popover.innerHTML = kinds.map(([ kind, label ], index) => (kind === "audiooutput" && !outputSelectable) ? "" : `
      <label class="huddle__field">${label}
        <select data-device="${kind}">
          ${options[index].map(device => `<option value="${escapeHTML(device.deviceId)}"${device.deviceId === active(kind) ? " selected" : ""}>${escapeHTML(device.label || `${label} ${device.deviceId.slice(0, 4)}`)}</option>`).join("") || `<option>No ${label.toLowerCase()} found</option>`}
        </select>
      </label>`).join("") + [
      [ "noiseSuppression", "Noise suppression" ],
      [ "echoCancellation", "Echo cancellation" ],
      [ "autoGainControl", "Automatic volume" ],
    ].map(([ name, label ]) => `
      <label class="huddle__toggle"><input type="checkbox" data-processing="${name}"${this.#prefs[name] ? " checked" : ""}> ${label}</label>`).join("") +
      `<p class="huddle__hint">⌘⇧Space (Ctrl+Shift+Space) mutes and unmutes.</p>`
  }

  #flash(text) {
    this.#status = text
    this.#render()
    setTimeout(() => {
      if (this.#status === text) {
        this.#status = ""
        this.#render()
      }
    }, 5000)
  }
}

export const huddle = new Huddle()

// Who's in each huddle of your rooms (HuddlesChannel), by room id: for the room's huddle button
// and the sidebar's 🎧. One subscription per page, kept across Turbo visits.
export const huddles = new Map()
let huddlesChannel

export async function trackHuddles() {
  if (huddlesChannel || !window.Current?.user) return
  huddlesChannel = await cable.subscribeTo({ channel: "HuddlesChannel" }, {
    received(data) {
      if (data.huddles) {
        huddles.clear()
        Object.entries(data.huddles).forEach(([ roomId, participants ]) => huddles.set(roomId, participants))
      } else if (data.participants.length) {
        huddles.set(String(data.room_id), data.participants)
      } else {
        huddles.delete(String(data.room_id))
      }
      showHuddles()
    }
  })
}

export function showHuddles() {
  document.querySelectorAll("#sidebar [data-room-id]").forEach(link => {
    const live = huddles.has(link.dataset.roomId)
    link.classList.toggle("room--huddle", live)
    if (live) link.title = `Huddle: ${huddles.get(link.dataset.roomId).map(person => person.name).join(", ")}`
    else link.removeAttribute("title")
  })
  huddle.dispatchEvent(new Event("change"))
}

// ⌘⇧Space / Ctrl+Shift+Space: mute and unmute, wherever you are.
document.addEventListener("keydown", event => {
  if (event.code === "Space" && event.shiftKey && (event.metaKey || event.ctrlKey) && huddle.connected) {
    event.preventDefault()
    huddle.toggleMic()
  }
  if (event.key === "Escape") document.querySelectorAll("#huddle [data-huddle-popover]").forEach(popover => { popover.hidden = true })
})

// Leaving the page leaves the call cleanly.
window.addEventListener("pagehide", () => huddle.connected && huddle.leave({ quietly: true }))
