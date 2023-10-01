let element = document.querySelector('input.force-dark-mode')
let dark_mode_query = window.matchMedia('(prefers-color-scheme: dark)')

let storage_is_dark_mode = localStorage.getItem('force-dark-mode')
let system_is_dark_mode = dark_mode_query.matches

force_dark_mode(storage_is_dark_mode === null ? system_is_dark_mode : storage_is_dark_mode === 'true')

element.addEventListener('change', () => force_dark_mode(element.checked))
dark_mode_query.addEventListener('change', e => force_dark_mode(e.matches))

export function force_dark_mode(value)
{
 document.body.classList.remove(`force-dark-mode-${!value}`)
 document.body.classList.add(`force-dark-mode-${value}`)
 element.checked = value
 localStorage.setItem('force-dark-mode', value)
}

window.force_dark_mode = force_dark_mode
