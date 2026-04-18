function init()
    error("Hello from init!")
end

function pos(cn, pos, event)
    info("received " .. pos.client_num .. " pos.x: " .. pos.pos.x)
end
