function init()
    error("Hello from init!")
end

function pos(cn, pos, event)
    info("received " .. pos.client_num .. " pos.x: " .. pos.pos.x)
    server:send_serv_msg(cn, { text = "nigga just moved " .. pos.pos.x })
end
