package net.velcore.hyperion_client.ui.login

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import de.connect2x.trixnity.clientserverapi.client.MatrixClientAuthProviderData
import kotlinx.coroutines.launch
import net.velcore.hyperion_client.account.AccountManager

class LoginViewModel(private val accountManager: AccountManager): ViewModel() {
    fun login(authData: MatrixClientAuthProviderData) {
        viewModelScope.launch { accountManager.login(authData) }
    }
}