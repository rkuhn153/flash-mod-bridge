# Papa’s Freezeria + mod bridge

Coolmath’s online Freezeria uses **AwayFL**, not Ruffle. For **live** money/day/rank:

1. Grab SWF:  
   `https://www.coolmathgames.com/sites/default/files/public_games/48987/assets/papasfreezeria_sdk_coolmath.swf`
2. Build this Ruffle fork (`web` selfhosted) and put dist in `mod-bridge/host/ruffle/`.
3. Open `mod-bridge/host/index.html` (static server), load the SWF.
4. MCP: `ping_flash_bridge` → `flash_find(keywords="allmoney,tips,score,day,rank")`
5. `flash_set_so_prop` / `flash_set` on the hit path — **no reload**.

Save key names may look like `//papasfreezeria_1` once SharedObject is created (start/continue once).
