// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A narrow no-break space. Unlike a comma or a dot it is never read as a decimal mark,
// and decimals here are always written with a dot whatever the language.
export const DIGIT_GROUP_SEPARATOR = '\u202F'
// Four digits stay together, so an ordinary fan speed reads as before.
const GROUPED_DIGITS_MIN = 5

// Groups the integer part of a long number in threes: 438300 reads 438 300.
export const groupDigits = (value: number | string): string => {
    const text = String(value)
    const match = /^(-?)(\d+)(\.\d*)?$/.exec(text)
    if (match == null) return text
    const [, sign, integer, fraction = ''] = match
    if (integer.length < GROUPED_DIGITS_MIN) return text
    return sign + integer.replace(/\B(?=(\d{3})+$)/g, DIGIT_GROUP_SEPARATOR) + fraction
}

// Reverses groupDigits. Also takes the ordinary spaces a pasted number may carry.
export const ungroupDigits = (text: string): string => text.replace(/\s/g, '')
