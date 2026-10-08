# The Vercel Episode — ELI5 Notes

*What happened, why we left, and what's on Cloudflare now.*
*Written 2026-10-08. Nothing here is secret — safe to share.*

## The short version

We put the Indy_Reads voice demo on Vercel (a website hosting company)
because the hackathon started there. It worked. Then Vercel kept getting
in the way: it couldn't run Rust, it capped our uploads, and its settings
page blocked our automation. So we moved everything to Cloudflare, where
our other stuff already lives. Vercel is now empty. Nothing was lost.

## What is Vercel? (like you're five)

Imagine you drew a picture and want to show it to the world. Vercel is
like a fridge door: you hand them the picture, they stick it up where
everyone can see it. Super easy. But:

- The fridge door is small. Big pictures don't fit. (250MB limit —
  our Rust/WASM work couldn't fit the way we wanted.)
- You can only use crayons. No paint, no clay. (JavaScript only —
  Zack's "why isn't this shit all fucking rust" was about this.)
- If you want to change the magnets holding the picture, you have to
  ask a grown-up at the company. (Their settings API blocked our
  automation token — we couldn't update config without clicking.)

## What is Cloudflare? (like you're five)

Cloudflare is like owning the whole art gallery instead of borrowing
fridge space. You hang the pictures, you run the lights, you keep the
keys. Our gallery already has:

- **ai-router** — the brain's phone line (calls NVIDIA, OpenAI, etc.)
- **lucidota-exchange** — the compute marketplace
- **odin-map** — the map thing
- **ctcc-router** — the heartbeat receiver

Now **indy-reads** lives there too. One gallery, one set of keys.

## How the move worked (the clever bits)

**The problem:** Vercel speaks "Node.js" (one dialect of JavaScript).
Cloudflare speaks "Workers" (a slightly different dialect). Our 5 backend
files needed translating.

**The trick:** A worker translated all 5 files automatically, keeping
every rule identical — the governor still vetoes the same way, receipts
still chain the same way. Then we glued them into ONE file (Cloudflare
likes single files) and uploaded it.

**The key problem we SOLVED:** On Vercel, the secret password (router
key) had to be smuggled inside the upload bundle because Vercel's
settings page blocked us. Hacky. On Cloudflare, we just... set it as
a proper secret via the API. Clean. This was the original sin that
started the whole "Vercel is fighting us" saga, and it's gone.

**Static files:** The pictures and pages (HTML, art, the WASM governor)
went into Cloudflare KV — think of it as a toy box the worker reaches
into when someone asks for a page.

## What we learned

1. **Borrowed fridge doors have rules.** Free tiers are generous until
   they aren't. The 250MB cap and 10-second timeout were fine for a
   demo, fatal for ambition.
2. **Automation matters.** If a platform won't let your robot do the
   clicking, you'll be clicking forever. Vercel's API said no; we left.
3. **One gallery beats five fridges.** Everything on Cloudflare means
   one login, one bill ($0), one set of keys, one place to look when
   something breaks.
4. **Rust was right.** The WASM governor (153KB of pure Rust) runs the
   same on Cloudflare as it does in the browser. JavaScript was just
   the delivery truck.

## What's in this zip

- `web/` — the demo pages, art, voice code, WASM governor (as served)
- `api/` — the ORIGINAL Vercel backend files (for the record)
- `cf-pages/` — the Cloudflare version: translated functions, the
  single worker file, KV uploader, deploy scripts
- `wasm/governor-wasm/` — the Rust source for the governor
- This file.

## Status as of 2026-10-08

- ✅ Code migrated and deployed to Cloudflare (worker `indy-reads`)
- ✅ Assets in KV, secrets set
- ⏳ Public URL pending (workers.dev route needs one dashboard toggle)
- ⏸️ Vercel project kept LIVE until Cloudflare URL is verified —
  nothing was deleted, nothing was lost.
