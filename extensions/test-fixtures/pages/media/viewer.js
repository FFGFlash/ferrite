ferrite.onMessage(message => {
  document.getElementById('size').textContent = `${message.size} bytes`
})
ferrite.postMessage({ type: 'size' })
