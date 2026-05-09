package dev.securechat.android.core

object SecureChatInput {
    private val schemePattern = Regex("^[A-Za-z][A-Za-z0-9+.-]*://")
    private val supportedSchemes = listOf("https://", "http://", "quic://")

    fun normalizeRelayUrl(value: String): String {
        var text = value.trim().replace(Regex("\\s+"), "")
        if (text.isEmpty()) return text

        for (outer in supportedSchemes) {
            for (inner in supportedSchemes) {
                val doubled = outer + inner
                if (text.startsWith(doubled, ignoreCase = true)) {
                    text = inner + text.substring(doubled.length)
                }
            }
        }

        return if (schemePattern.containsMatchIn(text)) text else "https://$text"
    }
}
