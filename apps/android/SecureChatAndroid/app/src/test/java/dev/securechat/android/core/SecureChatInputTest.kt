package dev.securechat.android.core

import org.junit.Assert.assertEquals
import org.junit.Test

class SecureChatInputTest {
    @Test
    fun normalizeRelayUrlKeepsSupportedSchemes() {
        assertEquals("https://chat.example.com", SecureChatInput.normalizeRelayUrl(" https://chat.example.com "))
        assertEquals("quic://chat.example.com:443", SecureChatInput.normalizeRelayUrl("quic://chat.example.com:443"))
    }

    @Test
    fun normalizeRelayUrlAddsHttpsForBareHosts() {
        assertEquals("https://chat.example.com", SecureChatInput.normalizeRelayUrl("chat.example.com"))
        assertEquals("https://203.0.113.10", SecureChatInput.normalizeRelayUrl("203.0.113.10"))
    }

    @Test
    fun normalizeRelayUrlRecoversCommonPasteMistakes() {
        assertEquals("https://chat.example.com", SecureChatInput.normalizeRelayUrl("https://https://chat.example.com"))
        assertEquals("quic://chat.example.com:443", SecureChatInput.normalizeRelayUrl("https://quic://chat.example.com:443"))
    }
}
