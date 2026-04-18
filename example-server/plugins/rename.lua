-- Lua port of sauer-server/examples/basic.rs ChangingNamePlugin.
--
-- Spawns a background loop per connected client that repeatedly sends ServInfo
-- with a bouncing, randomly-coloured "sauer-rs" banner in the server
-- description.  Duplicate N_CONNECT packets (the client re-sends Connect on
-- every ServInfo it receives) are blocked so BasePlugin never sees them a
-- second time.

local tasks = {}  -- [cn] = TaskHandle

-- ── colour helpers ────────────────────────────────────────────────────────────

-- cube-string colour escape bytes (values 0-8, preceded by 0x0C)
local COLOURS = { "\f0", "\f1", "\f2", "\f3", "\f4", "\f5", "\f6", "\f7", "\f8" }

local function random_colour()
    return COLOURS[math.random(#COLOURS)]
end

-- Bounce a position back and forth: 0 → n → 0 → n → …
local function triangle(i, n)
    local period = 2 * n
    local x = i % period
    return x <= n and x or (period - x)
end

-- ── callbacks ─────────────────────────────────────────────────────────────────

function connect(cn, packet, event)
    -- Duplicate Connect (client responded to one of our looping ServInfo
    -- packets): block it so BasePlugin doesn't welcome the player again.
    if tasks[cn] then
        event:cancel()
        return
    end

    -- First Connect for this client: pass through to BasePlugin, then start
    -- the animation loop.
    local i = 0
    local TOTAL_WIDTH = 20
    local TEXT_WIDTH  = 8

    tasks[cn] = server:spawn_loop(100, function()
        local pos  = triangle(i, TOTAL_WIDTH - TEXT_WIDTH - 1)
        local desc = string.rep(" ", pos)
                  .. random_colour() .. "sauer"
                  .. random_colour() .. "-"
                  .. random_colour() .. "rs"
                  .. string.rep(" ", TOTAL_WIDTH - TEXT_WIDTH - pos)
        i = i + 1

        server:send_serv_info(cn, {
            client_num   = cn,
            protocol     = PROTOCOL_VERSION,
            session_id   = 0,
            has_password = false,
            server_desc  = desc,
            server_auth  = "",
        })
    end)
end

function disconnected(cn, event)
    if tasks[cn] then
        tasks[cn]:cancel()
        tasks[cn] = nil
    end
end
