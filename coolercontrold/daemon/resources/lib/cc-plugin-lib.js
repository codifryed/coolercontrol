/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! This is a library for CoolerControl plugins
//! It provides a set of functions to interact with the CoolerControl app
//! It is provided by the daemon for all plugins, and just needs it linked in the plugin's HTML file
//! i.e.: <script type="text/javascript" src="../../cc-plugin-lib.js"></script>

// Internal Variables
/////////////////////////////////////////////////////////////
let _messageCount = 0
let _successfulConfigSaveCallback = () => {}
let _pluginConfig = {}
const _modes = []
const _alerts = []
const _profiles = []
const _functions = []
const _devices = []
let _status = new Map()

const _increaseMessageCount = () => _messageCount++
const _decreaseMessageCount = () => {
    if (_messageCount > 0) {
        _messageCount--
    }
    // dispatch event asynchronously
    setTimeout(() => window.dispatchEvent(new CustomEvent('decreasedMessageCount')), 0)
}

// Utility Functions
/////////////////////////////////////////////////////////////
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const waitTillAllMessagesReceived = async () => {
    while (_messageCount !== 0) {
        await new Promise((resolve) =>
            window.addEventListener('decreasedMessageCount', resolve, { once: true }),
        )
    }
}

// Message Processing Functions
/////////////////////////////////////////////////////////////
const _processMessages = (messageEvent) => {
    // console.log("iframe message received:", event);
    // The parent window messages do send an origin, even in a sandbox, so we only accept and send messages to same-origin
    if (!messageEvent.isTrusted || messageEvent.origin !== document.location.origin) {
        console.debug('Received invalid message', messageEvent)
        return
    }
    if (messageEvent.data == null) {
        console.log('message event data is null', messageEvent)
        return
    }
    switch (messageEvent.data.type) {
        case 'style':
            if (messageEvent.data.body == null) break
            const linkEl = document.createElement('link')
            linkEl.rel = 'stylesheet'
            linkEl.href = messageEvent.data.body
            document.head.appendChild(linkEl)
            break
        case 'customStyle':
            if (messageEvent.data.body == null) break
            Object.entries(messageEvent.data.body).forEach(([key, value]) =>
                document.documentElement.style.setProperty(key, value),
            )
            break
        case 'config':
            if (messageEvent.data.body == null) break
            _pluginConfig = messageEvent.data.body
            break
        case 'configSaved':
            if (messageEvent.data.body == null) {
                console.warn('Plugin config was not successfully saved')
                break
            }
            console.debug('Plugin config successfully saved', messageEvent.data.body)
            _pluginConfig = messageEvent.data.body
            _successfulConfigSaveCallback()
            break
        case 'modes':
            if (messageEvent.data.body == null) break
            _modes.length = 0
            _modes.push(...messageEvent.data.body)
            break
        case 'alerts':
            if (messageEvent.data.body == null) break
            _alerts.length = 0
            _alerts.push(...messageEvent.data.body)
            break
        case 'profiles':
            if (messageEvent.data.body == null) break
            _profiles.length = 0
            _profiles.push(...messageEvent.data.body)
            break
        case 'functions':
            if (messageEvent.data.body == null) break
            _functions.length = 0
            _functions.push(...messageEvent.data.body)
            break
        case 'devices':
            if (messageEvent.data.body == null) break
            _devices.length = 0
            _devices.push(...messageEvent.data.body)
            break
        case 'status':
            if (messageEvent.data.body == null) break
            _status = messageEvent.data.body
            break
        default:
            console.log('Unknown message type', messageEvent)
    }
    _decreaseMessageCount()
}

// Core Plugin Functions
/////////////////////////////////////////////////////////////

/* Load CC parent Styles into the iframe asynchronously */
const loadParentStyles = () => {
    _increaseMessageCount()
    window.parent.postMessage({ type: 'style' }, document.location.origin)
    _increaseMessageCount()
    window.parent.postMessage({ type: 'customStyle' }, document.location.origin)
}

/* A convenience function for simulating a cached synchronous Plugin Config request */
const getPluginConfig = async (force = false) => {
    if (_pluginConfig.length > 0 && !force) return _pluginConfig
    _increaseMessageCount()
    window.parent.postMessage({ type: 'loadConfig' }, document.location.origin)
    await waitTillAllMessagesReceived()
    return _pluginConfig
}

/* Convert a FormData object to a JavaScript object. Also supports multi-value fields */
const convertFormToObject = (formData) => {
    const object = {}
    formData.forEach((value, key) => {
        if (!Reflect.has(object, key)) {
            object[key] = value
            return
        }
        if (!Array.isArray(object[key])) {
            object[key] = [object[key]]
        }
        object[key].push(value)
    })
    return object
}

/* A convenience function for saving the plugin config. Note: the pluginConfig must be a JSON Object */
const savePluginConfig = async (pluginConfig) => {
    _increaseMessageCount()
    window.parent.postMessage({ type: 'saveConfig', body: pluginConfig }, document.location.origin)
    await waitTillAllMessagesReceived()
}

const successfulConfigSaveCallback = async (callback) => {
    _successfulConfigSaveCallback = callback
}

/* Close the plugin modal. This will end the plugin session. */
const close = () => {
    window.parent.postMessage({ type: 'close' }, document.location.origin)
}

/* Restart the daemon & UI. This has the effect of applying any plugin changes to service configs. */
const restart = () => {
    window.parent.postMessage({ type: 'restart' }, document.location.origin)
}

// Data Exchange Functions
/////////////////////////////////////////////////////////////

/* A convenience function for simulating a cached synchronous Modes request */
const getModes = async (force = false) => {
    if (_modes.length > 0 && !force) return _modes
    _increaseMessageCount()
    window.parent.postMessage({ type: 'modes' }, document.location.origin)
    await waitTillAllMessagesReceived()
    return _modes
}

/* A convenience function for simulating a cached synchronous Alerts request */
const getAlerts = async (force = false) => {
    if (_alerts.length > 0 && !force) return _alerts
    _increaseMessageCount()
    window.parent.postMessage({ type: 'alerts' }, document.location.origin)
    await waitTillAllMessagesReceived()
    return _alerts
}

/* A convenience function for simulating a cached synchronous Profiles request */
const getProfiles = async (force = false) => {
    if (_profiles.length > 0 && !force) return _profiles
    _increaseMessageCount()
    window.parent.postMessage({ type: 'profiles' }, document.location.origin)
    await waitTillAllMessagesReceived()
    return _profiles
}

/* A convenience function for simulating a cached synchronous Functions request */
const getFunctions = async (force = false) => {
    if (_functions.length > 0 && !force) return _functions
    _increaseMessageCount()
    window.parent.postMessage({ type: 'functions' }, document.location.origin)
    await waitTillAllMessagesReceived()
    return _functions
}

/* A convenience function for simulating a cached synchronous Devices request */
const getDevices = async (force = false) => {
    if (_devices.length > 0 && !force) return _devices
    _increaseMessageCount()
    window.parent.postMessage({ type: 'devices' }, document.location.origin)
    await waitTillAllMessagesReceived()
    return _devices
}

/* A convenience function for simulating a cached synchronous Status request */
const getStatus = async (force = false) => {
    if (_status.length > 0 && !force) return _status
    _increaseMessageCount()
    window.parent.postMessage({ type: 'status' }, document.location.origin)
    await waitTillAllMessagesReceived()
    return _status
}

// Plugin Running Functions
/////////////////////////////////////////////////////////////

/* A convenience function for running the plugin's JavaScript in an async wrapper. */
const runPluginScript = (mainPluginFunction, loadParentStyle = true) => {
    ;(async () => {
        if (loadParentStyle) {
            loadParentStyles()
            await waitTillAllMessagesReceived()
        }
        await mainPluginFunction()
    })()
}

window.addEventListener('message', _processMessages)
