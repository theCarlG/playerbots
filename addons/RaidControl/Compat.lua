-- Compat.lua
-- WoW 1.12 / 1.14 compatibility shims.
-- Loaded before all other addon files. Polyfills APIs missing in 1.12 so
-- the rest of the addon code doesn't need to know which version it's running on.

-- Detect native CLEU BEFORE stubbing (used to pick the right trigger backend)
RC_NATIVE_CLEU = CombatLogGetCurrentEventInfo ~= nil

-- Raid/party detection
if not IsInRaid then
    IsInRaid = function()
        return GetNumRaidMembers() > 0
    end
end

if not GetNumSubgroupMembers then
    GetNumSubgroupMembers = function()
        return GetNumPartyMembers()
    end
end

-- Combat log: stub out so calls don't error; triggers are silently disabled on 1.12
if not CombatLogGetCurrentEventInfo then
    CombatLogGetCurrentEventInfo = function()
        return nil
    end
end

if not GetPlayerInfoByGUID then
    GetPlayerInfoByGUID = function()
        return nil, nil, nil, nil, nil, nil
    end
end

-- _G is blocked in WoW 1.12's addon sandbox; expose it via getglobal/setglobal
if not _G then
    _G = setmetatable({}, {
        __index    = function(t, k) return getglobal(k) end,
        __newindex = function(t, k, v) setglobal(k, v) end,
    })
end

-- UIDropDownMenu_SetWidth argument order is reversed in WoW 1.12: (width, frame) vs (frame, width)
if not RC_NATIVE_CLEU and UIDropDownMenu_SetWidth then
    local _orig_SetWidth = UIDropDownMenu_SetWidth
    UIDropDownMenu_SetWidth = function(frame, width)
        _orig_SetWidth(width, frame)
    end
end
