package dev.securechat.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.securechat.android.core.SecureChatViewModel
import dev.securechat.android.ui.SecureChatApp

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            val viewModel: SecureChatViewModel = viewModel()
            SecureChatApp(viewModel)
        }
    }
}
