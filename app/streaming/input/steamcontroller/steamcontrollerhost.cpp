// SPDX-License-Identifier: GPL-3.0-or-later
#include "steamcontrollerhost.h"

#include "SDL_compat.h"
#include <Limelight.h>

#include <algorithm>
#include <deque>
#include <mutex>
#include <vector>

namespace {

const int MAX_STEAM_SLOTS = 16;   // the gamepad mask is 16 bits wide
const size_t MAX_PENDING_STATES = 32;
const uint64_t INPUT_RETRY_MS = 1;
const uint64_t MAX_RESYNC_RETRY_MS = 16;
const uint64_t MAX_INPUT_HISTORY_MS = 100;
const size_t MAX_RETRY_BATCH = 8;

struct PendingState {
    int flags = 0;
    uint8_t lt = 0, rt = 0;
    short lsX = 0, lsY = 0, rsX = 0, rsY = 0;
    uint64_t queuedAt = 0;
};

struct Entry {
    std::shared_ptr<SteamControllerLink> link;
    int generation = 0;
    uint8_t number = 0;
    // The last state the link reported under this announcement, replayed when input resumes.
    bool haveState = false;
    int buttonFlags = 0;
    uint8_t lt = 0, rt = 0;
    short lsX = 0, lsY = 0, rsX = 0, rsY = 0;
    // A gyro sample reached the host under this announcement: it is zeroed on focus loss.
    bool gyroSeen = false;
    std::deque<PendingState> pending;
    uint64_t retryDue = UINT64_MAX;
    uint64_t retryDelay = INPUT_RETRY_MS;
    bool resync = false;         // history exceeded its budget; only the current state remains
    bool retired = false;        // no more physical input, but a failed neutral still needs delivery
    unsigned rejected = 0;
};

struct Route {
    std::shared_ptr<SteamControllerLink> link;
    int generation = 0;
};

struct Registry {
    std::mutex mutex;
    bool alive = false;
    bool suppressed = false;      // window unfocused with background gamepad input off
    uint16_t mask = 0;
    SteamControllerHost::Clock clock = SteamController::nowMs;
    std::vector<Entry> entries;   // announced links

    // Under mutex: the controller number this link's generation is announced as, or -1.
    int numberFor(const SteamControllerLink& link, int generation) const {
        for (const Entry& e : entries) {
            if (!e.retired && e.link.get() == &link && e.generation == generation) return e.number;
        }
        return -1;
    }
    // Under mutex: the announcement of this link's generation, or null.
    Entry* find(const SteamControllerLink& link, int generation) {
        for (Entry& e : entries) {
            if (!e.retired && e.link.get() == &link && e.generation == generation) return &e;
        }
        return nullptr;
    }
    // Under mutex: the link announced as this controller number (the first in single-controller mode).
    Route entryFor(uint16_t number) const {
        for (const Entry& e : entries) {
            if (!e.retired && e.number == number) return {e.link, e.generation};
        }
        return {};
    }
};

Registry& registry() {
    static Registry instance;
    return instance;
}

PendingState latestState(const Entry& e, uint64_t nowMs) {
    return {e.buttonFlags, e.lt, e.rt, e.lsX, e.lsY, e.rsX, e.rsY, nowMs};
}

int triggerRange(uint8_t value) { return value == 0 ? 0 : value < 0xF0 ? 1 : 2; }
bool sameEdges(const PendingState& a, const PendingState& b) {
    return a.flags == b.flags && triggerRange(a.lt) == triggerRange(b.lt) && triggerRange(a.rt) == triggerRange(b.rt);
}

int sendState(const Entry& e, uint16_t mask, const PendingState& state) {
    return LiSendMultiControllerEvent(e.number, mask, state.flags, state.lt, state.rt,
                                      state.lsX, state.lsY, state.rsX, state.rsY);
}

void resetPending(Entry& e) {
    e.pending.clear();
    e.retryDue = UINT64_MAX;
    e.retryDelay = INPUT_RETRY_MS;
    e.resync = false;
    e.rejected = 0;
}

void resyncPending(Entry& e, const PendingState& state, const char* reason) {
    SteamController::logf(SteamController::LogWarn,
        "Steam Controller player %u: input history %s (%u states); resync to current state, pending button edges discarded",
        e.number, reason, (unsigned)e.pending.size());
    e.pending.clear();
    e.pending.push_back(state);
    e.resync = true;
}

// Under the registry mutex. The common-c API only admits to its queue: no network wait.
// History is retried in order, with a per-dispatch bound so a congested slot cannot monopolize
// the service thread. A 1 ms retry deadline is a scheduling target, not a delivery guarantee.
uint64_t drainPending(Entry& e, uint16_t mask, uint64_t nowMs) {
    if (e.pending.empty()) return UINT64_MAX;
    if (nowMs < e.retryDue) return e.retryDue;
    if (!e.resync && nowMs >= e.pending.front().queuedAt && nowMs - e.pending.front().queuedAt >= MAX_INPUT_HISTORY_MS) {
        PendingState current = e.pending.back();
        current.queuedAt = nowMs;
        resyncPending(e, current, "expired");
    }
    for (size_t i = 0; i < MAX_RETRY_BATCH && !e.pending.empty(); i++) {
        if (sendState(e, mask, e.pending.front()) != 0) {
            e.rejected++;
            // Preserve the short transient retry, then back off degraded resync to avoid
            // hammering common-c (and its rejection log) during prolonged congestion.
            if (e.resync) e.retryDelay = std::min(e.retryDelay * 2, MAX_RESYNC_RETRY_MS);
            e.retryDue = nowMs + e.retryDelay;
            return e.retryDue;
        }
        e.pending.pop_front();
    }
    if (e.pending.empty()) {
        SteamController::logf(SteamController::LogInfo,
            "Steam Controller player %u: input retry recovered after %u rejected sends%s",
            e.number, e.rejected, e.resync ? " (current-state resync)" : "");
        resetPending(e);
        return UINT64_MAX;
    }
    e.retryDue = nowMs + INPUT_RETRY_MS;
    return e.retryDue;
}

void submitState(Entry& e, uint16_t mask, PendingState state) {
    const uint64_t nowMs = state.queuedAt;
    if (e.pending.empty()) {
        if (sendState(e, mask, state) == 0) return;  // unchanged healthy fast path
        e.rejected = 1;
        e.retryDue = nowMs + INPUT_RETRY_MS;
        e.pending.push_back(state);
        SteamController::logf(SteamController::LogInfo,
            "Steam Controller player %u: input deferred; retaining button edges for retry", e.number);
    }
    else if (e.resync || sameEdges(e.pending.back(), state)) {
        // Keep the oldest timestamp when replacing analog travel: fresh motion must not extend
        // the lifetime of an old press indefinitely. Different digital/trigger ranges never merge.
        state.queuedAt = e.pending.back().queuedAt;
        e.pending.back() = state;
    }
    else if (e.pending.size() == MAX_PENDING_STATES) {
        resyncPending(e, state, "overflowed");
    }
    else {
        e.pending.push_back(state);
    }
    drainPending(e, mask, nowMs);
    // The callback only wakes the runner (it does not take the link mutex). It remains valid
    // until the runner is joined, and shutdown disables these registry operations first.
    if (!e.pending.empty() && e.link->wake) e.link->wake();
}

}

SteamControllerHost::SteamControllerHost(Clock clock)
{
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    r.alive = true;
    r.clock = clock;
    r.entries.clear();
    r.suppressed = false;   // a new session starts from the focus notifications it receives
}

SteamControllerHost::~SteamControllerHost()
{
    shutdown();
}

void SteamControllerHost::shutdown()
{
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    r.alive = false;
    r.entries.clear();
}

void SteamControllerHost::setActiveGamepadMask(uint16_t mask)
{
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    r.mask = mask;
}

void SteamControllerHost::announce(uint8_t controllerNumber, const std::shared_ptr<SteamControllerLink>& link, int generation)
{
    if (controllerNumber >= MAX_STEAM_SLOTS || !link) return;
    const uint16_t capabilities = link->capabilities();
    const uint32_t supported = SteamControllerCore::supportedButtonFlags();
    Registry& r = registry();
    {
        std::lock_guard<std::mutex> lock(r.mutex);
        if (!r.alive) return;
        // A failed disconnect neutral belongs to the old occupant. It must never release a
        // replacement controller's buttons after the new arrival has been admitted.
        r.entries.erase(std::remove_if(r.entries.begin(), r.entries.end(), [&](const Entry& e) {
            return e.link == link || (e.retired && e.number == controllerNumber);
        }), r.entries.end());
        Entry entry;
        entry.link = link;
        entry.generation = generation;
        entry.number = controllerNumber;
        r.entries.push_back(std::move(entry));
        SDL_LogInfo(SDL_LOG_CATEGORY_APPLICATION,
                    "Sending controller arrival: player %d type %u caps 0x%x buttons 0x%x (Steam Controller %s, mask 0x%x)",
                    controllerNumber, LI_CTYPE_STEAM, capabilities, supported, link->name().c_str(), r.mask);
        LiSendControllerArrivalEvent(controllerNumber, r.mask, LI_CTYPE_STEAM, supported, capabilities);
    }
    // Resend state and pad DOWNs; attributes and battery right after the arrival when known
    // (the link calls back into linkAttributes()/linkBattery(), which look the number up).
    link->requestInputReset();
    link->replayForArrival(generation);
}

void SteamControllerHost::retire(const std::shared_ptr<SteamControllerLink>& link, uint8_t controllerNumber, uint16_t newMask)
{
    if (controllerNumber >= MAX_STEAM_SLOTS || !link) return;
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    r.entries.erase(std::remove_if(r.entries.begin(), r.entries.end(), [&](const Entry& e) { return e.link == link; }), r.entries.end());
    r.mask = newMask;
    if (!r.alive) return;
    Entry retired;
    retired.link = link;
    retired.number = controllerNumber;
    retired.retired = true;
    PendingState neutral;
    neutral.queuedAt = r.clock();
    submitState(retired, r.mask, neutral);
    if (!retired.pending.empty()) r.entries.push_back(std::move(retired));
}

void SteamControllerHost::unpackEvent(const void* data1, const void* data2, int& linkId, int& generation, int& kind)
{
    linkId = (int)(uintptr_t)data1;
    uintptr_t packed = (uintptr_t)data2;
    kind = (int)(packed >> 24);
    generation = (int)(packed & 0xFFFFFF);
}

void SteamControllerHost::pushEvent(const SteamControllerLink& link, int generation, EventKind kind)
{
    SDL_Event event = {};
    event.type = SDL_USEREVENT;
    event.user.code = SDL_CODE_STEAM_CONTROLLER_LINK;
    event.user.data1 = (void*)(uintptr_t)link.id();
    event.user.data2 = (void*)(uintptr_t)(((uintptr_t)kind << 24) | ((uintptr_t)generation & 0xFFFFFF));
    SDL_PushEvent(&event);
}

// ----- host callbacks -----

void SteamControllerHost::clSteamHaptic(uint16_t controllerNumber, uint8_t length, const uint8_t* report)
{
    if (controllerNumber >= MAX_STEAM_SLOTS || report == nullptr || length == 0) return;
    Route entry;
    {
        Registry& r = registry();
        std::lock_guard<std::mutex> lock(r.mutex);
        if (!r.alive) return;
        entry = r.entryFor(controllerNumber);
    }
    if (entry.link) entry.link->steamHaptic(report, length, SteamController::nowMs(), entry.generation);
}

void SteamControllerHost::clSteamSettings(uint16_t controllerNumber, uint8_t command, uint8_t count, const uint8_t* triples)
{
    // A Bluetooth LE link advertises LI_CCAP_STEAM_SETTINGS and relays the write to its firmware
    // (minus the reserved ids). USB and puck links do not take host settings writes, as in the
    // Android client; a host that sends them anyway is ignored.
    if (controllerNumber >= MAX_STEAM_SLOTS) return;
    Route entry;
    {
        Registry& r = registry();
        std::lock_guard<std::mutex> lock(r.mutex);
        if (!r.alive) return;
        entry = r.entryFor(controllerNumber);
    }
    if (entry.link && entry.link->hostSettings(command, count, triples, SteamController::nowMs(), entry.generation)) {
        return;
    }
    static bool logged;
    if (!logged) {
        logged = true;
        SDL_LogInfo(SDL_LOG_CATEGORY_APPLICATION,
                    "Steam Controller: ignoring host settings 0x%02x (%u triples) for player %u; only a Bluetooth LE link takes them (USB/puck links keep their own link settings)",
                    command, count, controllerNumber);
    }
}

bool SteamControllerHost::rumble(uint16_t controllerNumber, uint16_t lowFreqMotor, uint16_t highFreqMotor)
{
    if (controllerNumber >= MAX_STEAM_SLOTS) return false;
    Route entry;
    {
        Registry& r = registry();
        std::lock_guard<std::mutex> lock(r.mutex);
        if (!r.alive) return false;
        entry = r.entryFor(controllerNumber);
    }
    return entry.link && entry.link->rumble(lowFreqMotor, highFreqMotor, SteamController::nowMs(), entry.generation);
}

bool SteamControllerHost::setMotionEventState(uint16_t controllerNumber, uint8_t motionType, uint16_t reportRateHz)
{
    if (controllerNumber >= MAX_STEAM_SLOTS) return false;
    Route entry;
    {
        Registry& r = registry();
        std::lock_guard<std::mutex> lock(r.mutex);
        if (!r.alive) return false;
        entry = r.entryFor(controllerNumber);
    }
    if (!entry.link || !entry.link->setMotionRate(motionType, reportRateHz, entry.generation)) return false;
    SDL_LogInfo(SDL_LOG_CATEGORY_APPLICATION, "Steam Controller %s: motion type %u at %u Hz (host request)",
                entry.link->name().c_str(), motionType, reportRateHz);
    return true;
}

// ----- focus policy (main thread) -----

void SteamControllerHost::notifyWindowFocus(bool focused, bool backgroundGamepad)
{
    const bool suppress = !focused && !backgroundGamepad;
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    if (r.suppressed == suppress) return;
    r.suppressed = suppress;
    if (!r.alive) return;
    for (Entry& e : r.entries) {
        if (e.retired) continue;
        if (suppress) {
            // A button, stick or finger held across the focus change must not stay down on the
            // host: neutral state, and every touch of both pads cancelled.
            resetPending(e);
            PendingState neutral;
            neutral.queuedAt = r.clock();
            submitState(e, r.mask, neutral);
            LiSendControllerTouchEvent2((uint8_t)e.number, LI_TOUCH_EVENT_CANCEL_ALL, 0, 0, 0.0f, 0.0f, 0.0f);
            LiSendControllerTouchEvent2((uint8_t)e.number, LI_TOUCH_EVENT_CANCEL_ALL, 1, 0, 0.0f, 0.0f, 0.0f);
            if (e.gyroSeen) {
                // The host keeps the last gyro sample alive with fresh timestamps, so an always-on
                // gyro mapping would keep turning: an unfocused controller is not rotating as far
                // as the host is concerned (common-c's input stream sends zero gyro the same way).
                LiSendControllerMotionEvent((uint8_t)e.number, LI_MOTION_TYPE_GYRO, 0.0f, 0.0f, 0.0f);
            }
        }
        else {
            if (e.haveState) {
                // The controller may have moved while nothing was forwarded: resume from its last state.
                submitState(e, r.mask, latestState(e, r.clock()));
            }
            // The host cancelled every contact and the decoder's dedup moved on while nothing was
            // forwarded: the link's next report resends its state, a fresh DOWN for every finger
            // on the pads (held across the change or begun while unfocused) and the current
            // motion sample. Lock-free, so it is safe to request under the registry mutex.
            e.link->requestInputReset();
        }
    }
}

bool SteamControllerHost::inputSuppressed()
{
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    return r.suppressed;
}

// ----- listener (driver threads, link mutex held) -----

void SteamControllerHost::linkConnected(SteamControllerLink& link, int generation)
{
    pushEvent(link, generation, LinkConnected);
}

void SteamControllerHost::linkDisconnected(SteamControllerLink& link, int generation)
{
    {
        Registry& r = registry();
        std::lock_guard<std::mutex> lock(r.mutex);
        Entry* e = r.alive ? r.find(link, generation) : nullptr;
        if (e != nullptr) {
            // Invalidate history immediately on the HID thread, before the SDL removal event.
            resetPending(*e);
            e->retired = true;
            PendingState neutral;
            neutral.queuedAt = r.clock();
            submitState(*e, r.mask, neutral);
        }
    }
    pushEvent(link, generation, LinkDisconnected);
}

void SteamControllerHost::linkState(SteamControllerLink& link, int generation, int buttonFlags, uint8_t lt, uint8_t rt,
                                    short lsX, short lsY, short rsX, short rsY)
{
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    Entry* e = r.alive ? r.find(link, generation) : nullptr;
    if (e == nullptr) return;
    // Remembered even while held back, so a focus regain resumes from the current state.
    e->haveState = true;
    e->buttonFlags = buttonFlags; e->lt = lt; e->rt = rt;
    e->lsX = lsX; e->lsY = lsY; e->rsX = rsX; e->rsY = rsY;
    if (r.suppressed) return;
    submitState(*e, r.mask, latestState(*e, r.clock()));
}

uint64_t SteamControllerHost::serviceInput(uint64_t nowMs)
{
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    if (!r.alive) return UINT64_MAX;
    uint64_t next = UINT64_MAX;
    for (Entry& e : r.entries) {
        // Suppressed entries contain only the focus-loss neutral, never physical state.
        next = std::min(next, drainPending(e, r.mask, nowMs));
    }
    r.entries.erase(std::remove_if(r.entries.begin(), r.entries.end(), [](const Entry& e) {
        return e.retired && e.pending.empty();
    }), r.entries.end());
    return next;
}

void SteamControllerHost::linkTouch(SteamControllerLink& link, int generation, uint8_t touchpadIndex, uint8_t eventType,
                                    uint32_t pointerId, float x, float y, float pressure)
{
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    int number = r.alive ? r.numberFor(link, generation) : -1;
    if (number < 0 || r.suppressed) return;
    LiSendControllerTouchEvent2((uint8_t)number, eventType, touchpadIndex, pointerId, x, y, pressure);
}

void SteamControllerHost::linkMotion(SteamControllerLink& link, int generation, uint8_t motionType, float x, float y, float z)
{
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    Entry* e = r.alive ? r.find(link, generation) : nullptr;
    if (e == nullptr) return;
    if (motionType == LI_MOTION_TYPE_GYRO) e->gyroSeen = true;
    if (r.suppressed) return;
    LiSendControllerMotionEvent((uint8_t)e->number, motionType, x, y, z);
}

void SteamControllerHost::linkBattery(SteamControllerLink& link, int generation, uint8_t state, uint8_t percent)
{
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    int number = r.alive ? r.numberFor(link, generation) : -1;
    if (number < 0) return;
    LiSendControllerBatteryEvent((uint8_t)number, state, percent);
}

void SteamControllerHost::linkAttributes(SteamControllerLink& link, int generation, const SteamControllerAttributes& attributes)
{
    Registry& r = registry();
    std::lock_guard<std::mutex> lock(r.mutex);
    int number = r.alive ? r.numberFor(link, generation) : -1;
    if (number < 0) return;
    LI_STEAM_CONTROLLER_ATTRIBUTES li = attributes.toLi();
    int err = LiSendSteamControllerAttributesEvent((uint8_t)number, &li);
    SDL_LogInfo(SDL_LOG_CATEGORY_APPLICATION, "Steam Controller %s: firmware attributes sent for player %d (valid mask 0x%02x, rc %d)",
                link.name().c_str(), number, attributes.validMask, err);
}
