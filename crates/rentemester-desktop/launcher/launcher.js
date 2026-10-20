const form = document.querySelector('#connect');
const error = document.querySelector('#error');
function mode() { return new FormData(form).get('mode'); }
form.addEventListener('change', () => {
  document.querySelector('#profile-field').hidden = mode() === 'remote';
  document.querySelector('#name-field').hidden = mode() !== 'create';
  document.querySelector('#server-field').hidden = mode() !== 'remote';
});
form.addEventListener('submit', async (event) => {
  event.preventDefault();
  error.textContent = '';
  const selected = mode();
  const profile = document.querySelector('#profile').value;
  const server = document.querySelector('#server').value;
  const name = document.querySelector('#name').value;
  const controls = Array.from(form.querySelectorAll('input,button'));
  controls.forEach((element) => { element.disabled = true; });
  try {
    await window.__TAURI__.core.invoke('launch', { mode: selected, profile, server, name });
  } catch (failure) {
    error.textContent = String(failure);
  } finally {
    controls.forEach((element) => { element.disabled = false; });
  }
});
