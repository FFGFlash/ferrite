flareite.onMessage(message => {
  document.getElementById('size').textContent = `${message.size} bytes`
})
flareite.postMessage({ type: 'size' })
