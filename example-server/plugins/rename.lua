-- Lua port of sauer-server/examples/basic.rs ChangingNamePlugin.
--
-- Spawns a background loop per connected client that repeatedly sends ServInfo
-- with a bouncing, randomly-coloured "sauer-rs" banner in the server
-- description.  Duplicate N_CONNECT packets (the client re-sends Connect on
-- every ServInfo it receives) are blocked so BasePlugin never sees them a
-- second time. Each ServInfo must repeat the client's session id: the client
-- stores it again and hashes passwords with it, so 0 would break /setmaster.

local tasks = {}  -- [id] = TaskHandle

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

function connect(id, packet, event)
    -- Duplicate Connect (client responded to one of our looping ServInfo
    -- packets): block it so BasePlugin doesn't welcome the player again.
    if tasks[id] then
        event:cancel()
        return
    end

    -- First Connect for this client: pass through to BasePlugin, then start
    -- the animation loop.
    local i = 0
    local TOTAL_WIDTH = 20
    local TEXT_WIDTH  = 8

    tasks[id] = server:set_interval(100, function()
        local pos  = triangle(i, TOTAL_WIDTH - TEXT_WIDTH - 1)
        local desc = string.rep(" ", pos)
                  .. random_colour() .. "sauer"
                  .. random_colour() .. "-"
                  .. random_colour() .. "rs"
                  .. string.rep(" ", TOTAL_WIDTH - TEXT_WIDTH - pos)
        i = i + 1

        local cn = server:get_cn(id) or 0
        server:send_serv_info(id, {
            client_num   = cn,
            protocol     = PROTOCOL_VERSION,
            session_id   = server:get_session_id(id) or 0,
            has_password = false,
            server_desc  = desc,
            server_auth  = "",
        })
    end)
end

function disconnected(id, event)
    if tasks[id] then
        tasks[id]:cancel()
        tasks[id] = nil
    end
end
