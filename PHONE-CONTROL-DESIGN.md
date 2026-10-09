# Phone as Control Surface — design reference

**Date:** 2026-10-08 (design note; no implementation).
**Doctrine:** the phone is the control surface, never the dev machine.

Architecture rhyme from LiteBox round-3 research (`net-litebox-round3.md`
§2.1 N7): anentrypoint/litebox splits its agent sandbox into a session
daemon (holds the sessions, owns the lifecycle) + a thin terminal
emulator + a presenter (streams the session to the viewer). That split
maps directly onto the phone-control story: a session daemon holding
organelle/agent sessions server-side, with the Pixel running only the
thin presenter — no session state, no secrets, no compute on the device
beyond rendering and input. Design reference only (their code is a
sandbox stack, not ours); the shape to steal is daemon-owns-state,
phone-renders.

→ Source: `~/workspace/goals/graph-hacks-2026-context-forge-falkordb-agent/hidden_files/research/net-litebox-round3.md` §2.1.
