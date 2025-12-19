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

const _increaseMessageCount = () => _messageCount++
const _decreaseMessageCount = () => {
    if (_messageCount <= 0) return
    _messageCount--
}

// Utility Functions
/////////////////////////////////////////////////////////////
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const waitTillAllMessagesReceived = async () => {
    let waitCount = 0
    while (_messageCount !== 0 && waitCount < 100) {
        await sleep(100)
        waitCount++
    }
    if (waitCount >= 100) {
        console.warn('waitTillAllMessagesReceived timeout')
        _messageCount = 0
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
            _modes.length = 0
            _modes.push(...messageEvent.data.body)
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
    window.parent.postMessage({ type: 'style' }, document.location.origin)
    _increaseMessageCount()
}

/* A convenience function for simulating a cached synchronous Plugin Config request */
const getPluginConfig = async (force = false) => {
    if (_pluginConfig.length > 0 && !force) return _pluginConfig
    window.parent.postMessage({ type: 'loadConfig' }, document.location.origin)
    _increaseMessageCount()
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
    window.parent.postMessage({ type: 'saveConfig', body: pluginConfig }, document.location.origin)
    _increaseMessageCount()
    await waitTillAllMessagesReceived()
}

const successfulConfigSaveCallback = async (callback) => {
    _successfulConfigSaveCallback = callback
}

/* Close the plugin modal. This will end the plugin session. */
const close = () => {
    window.parent.postMessage({ type: 'close' }, document.location.origin)
}

// Data Exchange Functions
/////////////////////////////////////////////////////////////

/* A convenience function for simulating a cached synchronous Modes request */
const getModes = async () => {
    if (_modes.length > 0) return _modes
    window.parent.postMessage({ type: 'modes' }, document.location.origin)
    _increaseMessageCount()
    await waitTillAllMessagesReceived()
    return _modes
}

// Plugin Running Functions
/////////////////////////////////////////////////////////////

/* A convenience function for running the plugin's JavaScript in an async wrapper. */
const runPluginScript = (mainPluginFunction, loadParentStyle = true) => {
    ;(async () => {
        if (loadParentStyle) loadParentStyles()
        await mainPluginFunction()
    })()
}

window.addEventListener('message', _processMessages)
