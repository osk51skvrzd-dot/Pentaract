
import ListItem from '@suid/material/ListItem'
import ListItemButton from '@suid/material/ListItemButton'
import ListItemIcon from '@suid/material/ListItemIcon'
import ListItemText from '@suid/material/ListItemText'
import MenuMUI from '@suid/material/Menu'
import MenuItem from '@suid/material/MenuItem'
import IconButton from '@suid/material/IconButton'
import FileIcon from '@suid/icons-material/InsertDriveFileOutlined'
import FolderIcon from '@suid/icons-material/Folder'
import MoreVertIcon from '@suid/icons-material/MoreVert'
import DownloadIcon from '@suid/icons-material/Download'
import InfoIcon from '@suid/icons-material/Info'
import DeleteIcon from '@suid/icons-material/Delete'
import { createSignal, Show } from 'solid-js'
import { useNavigate, useParams } from '@solidjs/router'
import API from '../api'
import ActionConfirmDialog from './ActionConfirmDialog'
import FileInfoDialog from './FileInfo'

/**
 * @typedef {Object} FSListItemProps
 * @property {import("../api").FSElement} fsElement
 * @property {string} storageId
 * @property {() => {}} onDelete
 */

/**
 * @param {FSListItemProps} props
 */
const FSListItem = (props) => {
    const [moreAnchorEl, setMoreAnchorEl] = createSignal(null)

    const [isActionConfirmDialogOpened, setIsActionConfirmDialogOpened] =
        createSignal(false)

    const [isInfoDialogOpened, setIsInfoDialogOpened] = createSignal(false)

    const navigate = useNavigate()
    const params = useParams()

    const openMore = () => Boolean(moreAnchorEl())

    const handleCloseMore = () => {
        setMoreAnchorEl(null)
    }

    const handleNavigate = () => {
        if (!props.fsElement.is_file) {
            navigate(`/storages/${props.storageId}/files/${props.fsElement.path}`)
        }
    }

    const download = async () => {
        const blob = await API.files.download(
            params.id,
            props.fsElement.path
        )

        const href = URL.createObjectURL(blob)

        const a = Object.assign(document.createElement('a'), {
            href,
            style: 'display: none',
            download: props.fsElement.name,
        })

        document.body.appendChild(a)
        a.click()
        URL.revokeObjectURL(href)
        a.remove()
    }

    const openActionConfirmDialog = () => {
        handleCloseMore()
        setIsActionConfirmDialogOpened(true)
    }

    const closeActionConfirmDialog = () => {
        setIsActionConfirmDialogOpened(false)
    }

    // Удаление файла или папки
    const deleteFile = async () => {
        closeActionConfirmDialog()

        let path = props.fsElement.path

        // Папки в базе данных хранятся с "/" в конце пути.
        // Поэтому добавляем его перед отправкой DELETE-запроса.
        if (!props.fsElement.is_file) {
            path = path.replace(/\/+$/, '') + '/'
        }

        try {
            await API.files.deleteFile(params.id, path)
            await props.onDelete()
        } catch (err) {
            console.error('Delete error:', err)
        }
    }

    return (
        <>
            <ListItem disablePadding>
                <ListItemButton onClick={handleNavigate}>
                    <ListItemIcon>
                        <Show when={props.fsElement.is_file} fallback={<FolderIcon />}>
                            <Show when={props.fsElement.thumbnail} fallback={<FileIcon />}>
                                <img
                                    src={props.fsElement.thumbnail}
                                    alt=""
                                    width="64"
                                    height="64"
                                    loading="lazy"
                                    decoding="async"
                                    style={{
                                        width: '64px',
                                        height: '64px',
                                        'object-fit': 'cover',
                                        'flex-shrink': '0',
                                    }}
                                />
                            </Show>
                        </Show>
                    </ListItemIcon>

                    <ListItemText primary={props.fsElement.name} />
                </ListItemButton>

                <IconButton
                    onClick={(event) => {
                        setMoreAnchorEl(event.currentTarget)
                    }}
                >
                    <MoreVertIcon />
                </IconButton>
            </ListItem>

            <MenuMUI
                id="basic-menu"
                anchorEl={moreAnchorEl()}
                open={openMore()}
                onClose={handleCloseMore}
                MenuListProps={{ 'aria-labelledby': 'basic-button' }}
            >
                <MenuItem onClick={() => setIsInfoDialogOpened(true)}>
                    <ListItemIcon>
                        <InfoIcon fontSize="small" />
                    </ListItemIcon>
                    <ListItemText>Info</ListItemText>
                </MenuItem>

                <MenuItem onClick={download} disabled={!props.fsElement.is_file}>
                    <ListItemIcon>
                        <DownloadIcon />
                    </ListItemIcon>
                    <ListItemText>Download</ListItemText>
                </MenuItem>

                <MenuItem onClick={openActionConfirmDialog}>
                    <ListItemIcon>
                        <DeleteIcon />
                    </ListItemIcon>
                    <ListItemText>Delete</ListItemText>
                </MenuItem>
            </MenuMUI>

            <ActionConfirmDialog
                action="Delete"
                entity="file"
                actionDescription={`delete file ${props.fsElement.name}`}
                isOpened={isActionConfirmDialogOpened()}
                onConfirm={deleteFile}
                onCancel={closeActionConfirmDialog}
            />

            <FileInfoDialog
                file={props.fsElement}
                isOpened={isInfoDialogOpened()}
                onClose={() => setIsInfoDialogOpened(false)}
            />
        </>
    )
}

export default FSListItem
