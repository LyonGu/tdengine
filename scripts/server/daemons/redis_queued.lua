--redis_queued.lua
--redis消息队列处理
REDIS_QUEUED = {}
setmetatable(REDIS_QUEUED, {__index = _G})
local _ENV = REDIS_QUEUED

function deal_with_reply(reply)
    if not IS_TABLE(reply) then
        return
    end
    
    -- TRACE("__ REDIS_QUEUED:deal_with_reply() __ is %o ", reply)
    if reply.channel == REDIS_CHAT_CHANNEL_WORLD then
        CHAT_D.deal_with_new_chat(DECODE_JSON(reply.payload))
    elseif reply.channel == SUBSCRIBE_ROOM_DETAIL_RECEIVE then
        ROOM_D.redis_room_detail(DECODE_JSON(reply.payload))
    elseif reply.channel == REDIS_ACCOUNT_START_HIBERNATE then
        if not is_rid_vaild(reply.payload) then
            return
        end
        raise_issue(EVENT_ACCOUNT_START_HIBERNATE, reply.payload)
    elseif reply.channel == REDIS_ACCOUNT_END_HIBERNATE then
        if not is_rid_vaild(reply.payload) then
            return
        end
        raise_issue(EVENT_ACCOUNT_END_HIBERNATE, reply.payload)
    elseif reply.channel == REDIS_ACCOUNT_OBJECT_CONSTRUCT then
        if not is_rid_vaild(reply.payload) then
            return
        end
        raise_issue(EVENT_ACCOUNT_OBJECT_CONSTRUCT, reply.payload)
    elseif reply.channel == REDIS_ACCOUNT_OBJECT_DESTRUCT then
        if not is_rid_vaild(reply.payload) then
            return
        end
        raise_issue(EVENT_ACCOUNT_OBJECT_DESTRUCT, reply.payload)
    elseif reply.channel == REDIS_NOTIFY_ACCOUNT_OBJECT_DESTRUCT then
        if not is_rid_vaild(reply.payload) then
            return
        end
        raise_issue(EVENT_NOTIFY_ACCOUNT_OBJECT_DESTRUCT, reply.payload)   
    elseif reply.channel == REDIS_ACCOUNT_WAIT_LOGIN then
        if not is_rid_vaild(reply.payload) then
            return
        end
        raise_issue(EVENT_ACCOUNT_WAIT_LOGIN, reply.payload)
    elseif reply.channel == REDIS_USER_ENTER_WORLD then
        local data = DECODE_JSON(reply.payload)
        if not is_rid_vaild(data.rid) then
            return
        end
        raise_issue(EVENT_USER_OBJECT_CONSTRUCT, data.rid, data.server_id)
    elseif reply.channel == REDIS_USER_CONNECTION_LOST then
        if not is_rid_vaild(reply.payload) then
            return
        end
        raise_issue(EVENT_USER_CONNECTION_LOST, reply.payload)

    else
        local room_name, user_rid, cookie = string.match(reply.channel, MATCH_ROOM_MSG_CHANNEL_USER)
        if room_name and user_rid then
            ROOM_D.redis_dispatch_message(room_name, user_rid, cookie, reply.payload)
            return
        end
        
        local server_id, user_rid, cookie = string.match(reply.channel, MATCH_SERVER_MSG_USER)
        if server_id and user_rid then
            local user = find_object_by_rid(user_rid)
            if not user then
                return
            end
            local name, net_msg = pack_raw_message(reply.payload)
            if not net_msg then
                return
            end
            if get_message_type(name) == MESSAGE_LOGIC then
                oper_message(user, name, net_msg)
            else
                user:send_net_msg(net_msg)
            end
            del_message(net_msg)
            return
        end


        local server_id, cookie = string.match(reply.channel, MATCH_RESPONE_SERVER_INFO)
        if server_id and cookie then
            INTERNAL_COMM_D.notify_internal_result(cookie, reply.payload)
            return
        end

        local room_name, cookie = string.match(reply.channel, MATCH_ROOM_MSG_CHANNEL_USER)
        if room_name and cookie then

            return
        end
    end
end

function deal_with_respone_list(respone_list)
    for _,reply in ipairs(respone_list) do
        deal_with_reply(reply)
    end
end

local function time_update()
    local respone_list = REDIS_D.subs_get_reply()
    if respone_list ~= nil and #respone_list > 0 then
        deal_with_respone_list(respone_list)
    end
end

function create()
    set_timer(100, time_update, nil, true)
end

create()